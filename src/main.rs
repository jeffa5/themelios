use clap::Parser;
use report::Reporter;
use stateright::actor::ActorModel;
use stateright::Checker;
use stateright::CheckerBuilder;
use stateright::Model;

mod api_server;
mod model;
mod opts;
mod register;
mod report;
mod scheduler;

fn main() {
    let opts = opts::Opts::parse();

    let model = model::ModelCfg {
        schedulers: opts.schedulers,
        api_servers: opts.api_servers,
    }
    .into_actor_model()
    .checker()
    .threads(num_cpus::get());
    run(opts, model)
}

fn run(opts: opts::Opts, model: CheckerBuilder<ActorModel<register::MyRegisterActor>>) {
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
