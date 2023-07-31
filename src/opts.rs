use clap::Parser;

#[derive(Parser, Debug)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: SubCmd,

    /// The number of threads to run.
    /// Defaults to the number of CPUs the machine has, as reported by `num_cpus`.
    #[clap(long, short, global = true)]
    pub threads: Option<usize>,

    #[clap(long, short, global = true, default_value = "1")]
    pub apps_per_client: usize,

    #[clap(long, short, global = true, default_value = "1")]
    pub clients: usize,

    #[clap(long, short, global = true, default_value = "1")]
    pub schedulers: usize,

    #[clap(long, short, global = true, default_value = "1")]
    pub datastores: usize,

    #[clap(long, short, global = true, default_value = "1")]
    pub nodes: usize,

    #[clap(long, global = true, default_value = "8080")]
    pub port: u16,

    /// Max depth for the check run, 0 is no limit.
    #[clap(long, global = true, default_value = "0")]
    pub max_depth: usize,
}

#[derive(clap::Subcommand, Debug)]
pub enum SubCmd {
    Serve,
    CheckDfs,
    CheckBfs,
    CheckSimulation {
        #[clap(long)]
        seed: Option<u64>,
    },
}
