use clap::Parser;
use model_checked_orchestration::model;
use model_checked_orchestration::state::PodResource;
use model_checked_orchestration::state::ReplicaSetResource;
use model_checked_orchestration::state::State;
use report::Reporter;
use stateright::Checker;
use stateright::Model;
use stateright::UniformChooser;
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

    let initial_state = State::default()
        .with_pods((0..opts.initial_pods).map(|i| PodResource {
            id: i,
            node_name: None,
        }))
        .with_replicasets((1..=opts.replicasets).map(|i| ReplicaSetResource {
            id: i,
            replicas: opts.pods_per_replicaset,
        }));

    let model = model::OrchestrationModelCfg {
        initial_state,
        schedulers: opts.schedulers,
        nodes: opts.nodes,
        datastores: opts.datastores,
        replicaset_controllers: opts.replicaset_controllers,
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
    }
}
