use std::collections::BTreeMap;
use std::io::IsTerminal;

use clap::Parser;
use stateright::Checker;
use stateright::Model;
use stateright::UniformChooser;
use themelios::model;
use themelios::report::StdoutReporter;
use themelios::resources::Deployment;
use themelios::resources::DeploymentSpec;
use themelios::resources::DeploymentStatus;
use themelios::resources::LabelSelector;
use themelios::resources::Node;
use themelios::resources::NodeSpec;
use themelios::resources::NodeStatus;
use themelios::resources::Pod;
use themelios::resources::PodSpec;
use themelios::resources::PodStatus;
use themelios::resources::PodTemplateSpec;
use themelios::resources::ReplicaSet;
use themelios::resources::ReplicaSetSpec;
use themelios::resources::ReplicaSetStatus;
use themelios::resources::StatefulSet;
use themelios::resources::StatefulSetSpec;
use themelios::resources::StatefulSetStatus;
use themelios::state::history::ConsistencySetup;
use themelios::state::RawState;
use themelios::utils;
use tokio::runtime::Runtime;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing::metadata::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub mod opts;

fn main() {
    let opts = opts::Opts::parse();

    let is_terminal = std::io::stdout().is_terminal();
    let log_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::registry()
        .with(fmt::layer().with_ansi(is_terminal))
        .with(log_filter)
        .init();

    let initial_state = RawState::default()
        .with_pods((0..opts.initial_pods).map(|i| Pod {
            metadata: utils::metadata(format!("pod-{i}")),
            spec: PodSpec {
                node_name: None,
                scheduler_name: None,
                containers: Vec::new(),
                init_containers: Vec::new(),
                active_deadline_seconds: None,
                termination_grace_period_seconds: None,
                restart_policy: None,
                volumes: Vec::new(),
                hostname: String::new(),
                subdomain: String::new(),
                tolerations: Vec::new(),
                node_selector: BTreeMap::new(),
            },
            status: PodStatus::default(),
        }))
        .with_replicasets((1..=opts.replicasets).map(|i| ReplicaSet {
            metadata: utils::metadata(format!("rep-{i}")),
            spec: ReplicaSetSpec {
                replicas: Some(opts.pods_per_replicaset),
                template: PodTemplateSpec {
                    metadata: utils::metadata(format!("rep-{i}-container")),
                    spec: PodSpec {
                        node_name: None,
                        scheduler_name: None,
                        containers: Vec::new(),
                        init_containers: Vec::new(),
                        active_deadline_seconds: None,
                        termination_grace_period_seconds: None,
                        restart_policy: None,
                        volumes: Vec::new(),
                        hostname: String::new(),
                        subdomain: String::new(),
                        tolerations: Vec::new(),
                        node_selector: BTreeMap::new(),
                    },
                },
                min_ready_seconds: 0,
                selector: LabelSelector {
                    match_labels: Default::default(),
                },
            },
            status: ReplicaSetStatus::default(),
        }))
        .with_deployments((1..=opts.deployments).map(|i| Deployment {
            metadata: utils::metadata(format!("dep-{i}")),
            spec: DeploymentSpec {
                replicas: opts.pods_per_replicaset,
                template: PodTemplateSpec {
                    metadata: utils::metadata(format!("dep-{i}-container")),
                    spec: PodSpec {
                        node_name: None,
                        scheduler_name: None,
                        containers: Vec::new(),
                        init_containers: Vec::new(),
                        active_deadline_seconds: None,
                        termination_grace_period_seconds: None,
                        restart_policy: None,
                        volumes: Vec::new(),
                        hostname: String::new(),
                        subdomain: String::new(),
                        tolerations: Vec::new(),
                        node_selector: BTreeMap::new(),
                    },
                },
                min_ready_seconds: 0,
                selector: LabelSelector {
                    match_labels: BTreeMap::default(),
                },
                paused: false,
                revision_history_limit: 0,
                strategy: None,
                progress_deadline_seconds: None,
            },
            status: DeploymentStatus::default(),
        }))
        .with_statefulsets((1..=opts.statefulsets).map(|i| StatefulSet {
            metadata: utils::metadata(format!("sts-{i}")),
            spec: StatefulSetSpec {
                replicas: Some(opts.pods_per_statefulset),
                ..Default::default()
            },
            status: StatefulSetStatus::default(),
        }))
        .with_nodes((0..opts.nodes).map(|i| Node {
            metadata: utils::metadata(format!("node-{i}")),
            spec: NodeSpec {
                taints: Vec::new(),
                unschedulable: false,
            },
            status: NodeStatus::default(),
        }));

    let consistency_level = if opts.session {
        ConsistencySetup::ResettableSession
    } else if opts.optimistic_linear {
        ConsistencySetup::OptimisticLinear
    } else if opts.causal {
        ConsistencySetup::Causal
    } else {
        // default to synchronous
        ConsistencySetup::Synchronous
    };
    let model = model::OrchestrationModelCfg {
        initial_state,
        consistency_level,
        schedulers: opts.schedulers,
        nodes: opts.nodes,
        replicaset_controllers: opts.replicaset_controllers,
        deployment_controllers: opts.deployment_controllers,
        statefulset_controllers: opts.statefulset_controllers,
        job_controllers: opts.job_controllers,
        podgc_controllers: opts.podgc_controllers,
        properties: Vec::new(),
    };
    run(opts, model.into_abstract_model())
}

fn run<M>(opts: opts::Opts, model: M)
where
    M: Model + Send + Sync + 'static,
    M::State: Send + Sync + std::hash::Hash + std::fmt::Debug + Clone,
    M::Action: Send + Sync + std::hash::Hash + std::fmt::Debug + Clone,
{
    println!("Running with config {:?}", opts);
    let mut reporter = StdoutReporter::new(&model);
    let threads = opts.threads.unwrap_or_else(num_cpus::get);
    let checker = model
        .checker()
        .target_max_depth(opts.max_depth)
        .threads(threads);

    match opts.command {
        opts::SubCmd::Explore {
            port,
            fingerprint_path,
        } => {
            let path = fingerprint_path
                .map(|p| format!("/#/steps/{}", p))
                .unwrap_or_default();
            println!("Serving web ui on http://127.0.0.1:{}{}", port, path);
            checker.serve(("127.0.0.1", port));
        }
        opts::SubCmd::CheckDfs => {
            checker.spawn_dfs().report(&mut reporter).join();
        }
        opts::SubCmd::CheckBfs => {
            checker.spawn_bfs().report(&mut reporter).join();
        }
        opts::SubCmd::CheckSimulation { seed } => {
            let seed = seed.unwrap_or(0);
            checker
                .spawn_simulation(seed, UniformChooser)
                .report(&mut reporter)
                .join();
        }
        opts::SubCmd::ServeTest { port } => {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let trace_layer = TraceLayer::new_for_http();
                let app = themelios::serve_test::app().layer(trace_layer);
                let address = format!("127.0.0.1:{port}");
                info!("Serving test API on {address}");
                let listener = tokio::net::TcpListener::bind(address).await.unwrap();
                axum::serve(listener, app).await.unwrap();
            });
        }
        opts::SubCmd::ServeCluster { port } => {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let address = format!("127.0.0.1:{port}");
                info!("Serving cluster API on {address}");
                let (shutdown, handles) = themelios::serve_cluster::run(address).await;
                tokio::signal::ctrl_c().await.unwrap();
                shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
                for handle in handles {
                    handle.await.unwrap();
                }
            });
        }
        opts::SubCmd::ControllerManager {} => {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                info!("Serving controllers");
                let (shutdown, handles) = themelios::controller_manager::run().await;
                tokio::signal::ctrl_c().await.unwrap();
                shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
                for handle in handles {
                    handle.await.unwrap();
                }
            });
        }
    }
}
