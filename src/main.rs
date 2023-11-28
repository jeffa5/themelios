use std::collections::BTreeMap;
use std::io::IsTerminal;

use clap::Parser;
use model_checked_orchestration::controller::client::ClientState;
use model_checked_orchestration::model;
use model_checked_orchestration::report::Reporter;
use model_checked_orchestration::resources::Deployment;
use model_checked_orchestration::resources::DeploymentSpec;
use model_checked_orchestration::resources::DeploymentStatus;
use model_checked_orchestration::resources::LabelSelector;
use model_checked_orchestration::resources::Node;
use model_checked_orchestration::resources::NodeSpec;
use model_checked_orchestration::resources::NodeStatus;
use model_checked_orchestration::resources::Pod;
use model_checked_orchestration::resources::PodSpec;
use model_checked_orchestration::resources::PodStatus;
use model_checked_orchestration::resources::PodTemplateSpec;
use model_checked_orchestration::resources::ReplicaSet;
use model_checked_orchestration::resources::ReplicaSetSpec;
use model_checked_orchestration::resources::ReplicaSetStatus;
use model_checked_orchestration::resources::StatefulSet;
use model_checked_orchestration::resources::StatefulSetSpec;
use model_checked_orchestration::resources::StatefulSetStatus;
use model_checked_orchestration::state::ConsistencySetup;
use model_checked_orchestration::state::StateView;
use model_checked_orchestration::utils;
use stateright::Checker;
use stateright::Model;
use stateright::UniformChooser;
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

    let initial_state = StateView::default()
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
        .with_nodes((0..opts.nodes).map(|i| {
            (
                i,
                Node {
                    metadata: utils::metadata(format!("node-{i}")),
                    spec: NodeSpec {
                        taints: Vec::new(),
                        unschedulable: false,
                    },
                    status: NodeStatus::default(),
                },
            )
        }))
        .with_controllers(
            opts.nodes
                ..(opts.schedulers
                    + opts.datastores
                    + opts.replicaset_controllers
                    + opts.deployment_controllers
                    + opts.statefulset_controllers),
        );

    let consistency_level = if let Some(k) = opts.bounded_staleness {
        ConsistencySetup::BoundedStaleness(k)
    } else if opts.session {
        ConsistencySetup::Session
    } else if opts.eventual {
        ConsistencySetup::Eventual
    } else if let Some(commit_every) = opts.optimistic_linear {
        ConsistencySetup::OptimisticLinear(commit_every)
    } else if opts.causal {
        ConsistencySetup::Causal
    } else {
        // default to strong
        ConsistencySetup::Strong
    };
    let model = model::OrchestrationModelCfg {
        initial_state,
        consistency_level,
        schedulers: opts.schedulers,
        nodes: opts.nodes,
        datastores: opts.datastores,
        replicaset_controllers: opts.replicaset_controllers,
        deployment_controllers: opts.deployment_controllers,
        statefulset_controllers: opts.statefulset_controllers,
        client_state: ClientState::new_ordered(),
        properties: Vec::new(),
    };
    if opts.actors {
        run(opts, model.into_actor_model())
    } else {
        run(opts, model.into_abstract_model())
    }
}

fn run<M>(opts: opts::Opts, model: M)
where
    M: Model + Send + Sync + 'static,
    M::State: Send + Sync + std::hash::Hash + std::fmt::Debug,
    M::Action: Send + Sync + std::hash::Hash + std::fmt::Debug,
{
    println!("Running with config {:?}", opts);
    let mut reporter = Reporter::new(&model);
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
        opts::SubCmd::Serve { port } => {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let trace_layer = TraceLayer::new_for_http();
                let app = model_checked_orchestration::serve::app().layer(trace_layer);
                let address = format!("127.0.0.1:{port}");
                info!("Serving on {address}");
                axum::Server::bind(&address.parse().unwrap())
                    .serve(app.into_make_service())
                    .await
                    .unwrap();
            });
        }
    }
}
