use std::collections::BTreeMap;

use clap::Parser;
use model_checked_orchestration::model;
use model_checked_orchestration::resources::DeploymentResource;
use model_checked_orchestration::resources::DeploymentSpec;
use model_checked_orchestration::resources::DeploymentStatus;
use model_checked_orchestration::resources::LabelSelector;
use model_checked_orchestration::resources::PodResource;
use model_checked_orchestration::resources::PodSpec;
use model_checked_orchestration::resources::PodStatus;
use model_checked_orchestration::resources::PodTemplateSpec;
use model_checked_orchestration::resources::ReplicaSetResource;
use model_checked_orchestration::resources::ReplicaSetSpec;
use model_checked_orchestration::resources::ReplicaSetStatus;
use model_checked_orchestration::resources::ResourceQuantities;
use model_checked_orchestration::resources::ResourceRequirements;
use model_checked_orchestration::resources::StatefulSetResource;
use model_checked_orchestration::state::ConsistencySetup;
use model_checked_orchestration::state::StateView;
use model_checked_orchestration::utils;
use report::Reporter;
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
pub mod report;

fn main() {
    let opts = opts::Opts::parse();

    let log_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::registry()
        .with(fmt::layer().with_ansi(true))
        .with(log_filter)
        .init();

    let initial_state = StateView::default()
        .with_pods((0..opts.initial_pods).map(|i| PodResource {
            metadata: utils::metadata(format!("pod-{i}")),
            spec: PodSpec {
                node_name: None,
                scheduler_name: None,
                resources: Some(ResourceRequirements {
                    requests: Some(ResourceQuantities {
                        cpu_cores: Some(2.into()),
                        memory_mb: Some(3000.into()),
                        pods: Some(32.into()),
                    }),
                    limits: None,
                }),
                containers: Vec::new(),
                active_deadline_seconds: None,
                termination_grace_period_seconds: None,
                restart_policy: None,
            },
            status: PodStatus {},
        }))
        .with_replicasets((1..=opts.replicasets).map(|i| ReplicaSetResource {
            metadata: utils::metadata(format!("rep-{i}")),
            spec: ReplicaSetSpec {
                replicas: Some(opts.pods_per_replicaset),
                template: PodTemplateSpec {
                    metadata: utils::metadata(format!("rep-{i}-container")),
                    spec: PodSpec {
                        node_name: None,
                        scheduler_name: None,
                        resources: None,
                        containers: Vec::new(),
                        active_deadline_seconds: None,
                        termination_grace_period_seconds: None,
                        restart_policy: None,
                    },
                },
                min_ready_seconds: 0,
                selector: LabelSelector {
                    match_labels: Default::default(),
                },
            },
            status: ReplicaSetStatus::default(),
        }))
        .with_deployments((1..=opts.deployments).map(|i| DeploymentResource {
            metadata: utils::metadata(format!("dep-{i}")),
            spec: DeploymentSpec {
                replicas: opts.pods_per_replicaset,
                template: PodTemplateSpec {
                    metadata: utils::metadata(format!("dep-{i}-container")),
                    spec: PodSpec {
                        node_name: None,
                        scheduler_name: None,
                        resources: None,
                        containers: Vec::new(),
                        active_deadline_seconds: None,
                        termination_grace_period_seconds: None,
                        restart_policy: None,
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
        .with_statefulsets((1..=opts.statefulsets).map(|i| StatefulSetResource {
            metadata: utils::metadata(format!("sts-{i}")),
            replicas: opts.pods_per_statefulset,
        }));

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
