use clap::Parser;
use report::Reporter;
use stateright::actor::ActorModel;
use stateright::Checker;
use stateright::CheckerBuilder;
use stateright::Model;

use model_checked_orchestration::model;
use model_checked_orchestration::opts;
use model_checked_orchestration::report;
use model_checked_orchestration::root;

fn main() {
    let opts = opts::Opts::parse();

    let model = model::ModelCfg {
        schedulers: opts.schedulers,
        nodes: opts.nodes,
        datastores: opts.datastores,
    }
    .into_actor_model()
    .checker()
    .threads(num_cpus::get());
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
    }
}
