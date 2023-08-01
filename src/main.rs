use clap::Parser;
use model_checked_orchestration::root::RootState;
use report::Reporter;
use stateright::actor::ActorModel;
use stateright::Checker;
use stateright::Model;
use stateright::UniformChooser;
use tracing::metadata::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use model_checked_orchestration::model;
use model_checked_orchestration::opts;
use model_checked_orchestration::report;
use model_checked_orchestration::root;

fn main() {
    let opts = opts::Opts::parse();

    let log_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::registry()
        .with(fmt::layer().with_ansi(true))
        .with(log_filter)
        .init();

    let model = model::ModelCfg {
        apps_per_client: opts.apps_per_client,
        clients: opts.clients,
        schedulers: opts.schedulers,
        nodes: opts.nodes,
        datastores: opts.datastores,
    }
    .into_actor_model()
    .property(
        // TODO: eventually properties don't seem to work with timers, even though they may be
        // steady state.
        stateright::Expectation::Eventually,
        "every application gets scheduled",
        |model, state| {
            let mut any = false;
            let total_apps = model.cfg.apps_per_client as usize * model.cfg.clients;
            for actor in &state.actor_states {
                if let RootState::Datastore(d) = &**actor {
                    if d.unscheduled_apps.is_empty() && d.scheduled_apps.len() == total_apps {
                        any = true;
                    }
                }
            }
            any
        },
    );
    run(opts, model)
}

fn run(opts: opts::Opts, model: ActorModel<root::Root, model::ModelCfg>) {
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
