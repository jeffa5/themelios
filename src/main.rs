use clap::Parser;
use model_checked_orchestration::root::RootState;
use report::Reporter;
use stateright::actor::ActorModel;
use stateright::Checker;
use stateright::CheckerBuilder;
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

    let threads = opts.threads.unwrap_or_else(|| num_cpus::get());

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
        |_model, state| {
            let mut all = true;
            for actor in &state.actor_states {
                if let RootState::Datastore(d) = &**actor {
                    if !d.unscheduled_apps.is_empty() {
                        all = false;
                    }
                }
            }
            all
        },
    )
    .checker()
    .threads(threads);
    run(opts, model)
}

fn run(opts: opts::Opts, model: CheckerBuilder<ActorModel<root::Root>>) {
    println!("Running with config {:?}", opts);
    match opts.command {
        opts::SubCmd::Serve => {
            println!("Serving web ui on http://127.0.0.1:{}", opts.port);
            model.serve(("127.0.0.1", opts.port));
        }
        opts::SubCmd::CheckDfs => {
            model
                .spawn_dfs()
                .report(&mut Reporter::default())
                .join()
                .assert_properties();
        }
        opts::SubCmd::CheckBfs => {
            model
                .spawn_bfs()
                .report(&mut Reporter::default())
                .join()
                .assert_properties();
        }
        opts::SubCmd::CheckSimulation { seed } => {
            let seed = seed.unwrap_or(0);
            model
                .spawn_simulation(seed, UniformChooser)
                .report(&mut Reporter::default())
                .join()
                .assert_properties();
        }
    }
}
