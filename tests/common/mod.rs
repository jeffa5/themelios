use stateright::Checker;
use stateright::HasDiscoveries;
use stateright::Model;
use stateright::UniformChooser;
use std::collections::BTreeMap;
use std::fs::create_dir;
use std::num::NonZeroU64;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;
use themelios::model::OrchestrationModelCfg;
use themelios::report::CSVReporter;
use themelios::report::JointReporter;
use themelios::report::StdoutReporter;
use themelios::state::history::ConsistencySetup;
use tracing::info;

macro_rules! test_table {
    { $globalname:ident, $name:ident($consistency:expr, $controllers:expr), } => {
        paste::item! {
            #[test_log::test]
            fn [< $globalname _ $name _ 100 >]() {
                let model = $globalname($consistency, $controllers);
                run(model, function_name!(), true, 100)
            }
            #[test_log::test]
            fn [< $globalname _ $name _ 200 >]() {
                let model = $globalname($consistency, $controllers);
                run(model, function_name!(), true, 200)
            }
        }
    };
    { $global_name:ident, $name:ident($consistency:expr, $controllers:expr), $($x:ident($y:expr, $z:expr)),+, } => {
        test_table! { $global_name, $name($consistency, $controllers), }
        test_table! { $global_name, $($x($y, $z)),+, }
    }
}

macro_rules! test_table_panic {
    { $globalname:ident, $name:ident($consistency:expr, $controllers:expr), } => {
        paste::item! {
            #[test_log::test]
            fn [< $globalname _ $name _ 100 >]() {
                let model = $globalname($consistency, $controllers);
                run(model, function_name!(), false, 100)
            }
            #[test_log::test]
            fn [< $globalname _ $name _ 200 >]() {
                let model = $globalname($consistency, $controllers);
                run(model, function_name!(), false, 200)
            }
        }
    };
    { $global_name:ident, $name:ident($consistency:expr, $controllers:expr), $($x:ident($y:expr, $z:expr)),+, } => {
        test_table_panic! { $global_name, $name($consistency, $controllers), }
        test_table_panic! { $global_name, $($x($y, $z)),+, }
    }
}

pub(crate) use test_table;
pub(crate) use test_table_panic;

pub fn run(model: OrchestrationModelCfg, fn_name: &str, should_succeed: bool, max_depth: usize) {
    println!("Running test {:?}", fn_name);

    if let Ok(explore_test) = std::env::var("MCO_EXPLORE_TEST") {
        if fn_name.contains(&explore_test) {
            let path = std::env::var("MCO_EXPLORE_PATH").unwrap_or_default();
            explore(model, path);
        }
        // skip others
    } else {
        check(model, fn_name, should_succeed, max_depth)
    }
}

fn check(model: OrchestrationModelCfg, test_name: &str, should_succeed: bool, max_depth: usize) {
    println!("Checking model");
    let consistency = model.consistency_level.clone();
    let controllers = model.nodes;
    let am = model.into_abstract_model();
    let report_dir =
        PathBuf::from(std::env::var("MCO_REPORT_PATH").unwrap_or_else(|_| "testout".to_owned()));
    if !report_dir.exists() {
        create_dir(&report_dir).unwrap();
    } else if !report_dir.is_dir() {
        panic!("Report dir {report_dir:?} should be a directory!");
    }
    let report_file = format!("{test_name}.csv");
    let report_path = report_dir.join(report_file);
    let depths = DepthTracker::new(
        max_depth,
        consistency.clone(),
        controllers,
        test_name.to_owned(),
    );
    let depths2 = depths.clone();
    let mut reporter = JointReporter {
        reporters: vec![
            Box::new(StdoutReporter::new(&am)),
            Box::new(CSVReporter::new(
                &report_path,
                consistency,
                controllers,
                test_name.to_owned(),
            )),
        ],
    };
    let checker = am
        .checker()
        .terminal_visitor(depths)
        .threads(num_cpus::get())
        .finish_when(HasDiscoveries::AnyFailures)
        .target_max_depth(max_depth)
        .timeout(Duration::from_secs(60));
    let check_mode = std::env::var("MCO_CHECK_MODE").unwrap_or_else(|_| String::new());
    #[allow(clippy::wildcard_in_or_patterns)]
    let check_result = match check_mode.as_str() {
        "dfs" => {
            info!(check_mode, "Running checking");
            checker.spawn_dfs().report(&mut reporter).check_properties()
        }
        "bfs" => {
            info!(check_mode, "Running checking");
            checker.spawn_bfs().report(&mut reporter).check_properties()
        }
        "simulation" | _ => {
            info!(check_mode, "Running checking");
            checker
                .spawn_simulation(0, UniformChooser)
                .report(&mut reporter)
                .check_properties()
        }
    };
    let depth_file = format!("{test_name}-depths.csv");
    depths2.to_csv(&report_dir.join(depth_file));
    if check_result.iter().all(|(_, ok)| *ok) != should_succeed && !cfg!(tarpaulin) {
        // don't panic during coverage runs, that breaks the llvm engine
        panic!("Some properties failed");
    }
}

fn explore(model: OrchestrationModelCfg, mut path: String) {
    let host = "127.0.0.1";
    let port = 8080;
    if !path.is_empty() {
        path = format!("#/steps/{path}");
    }
    println!(
        "Exploring model, served on http://{}:{}/{}",
        host, port, path
    );
    let am = model.into_abstract_model();
    am.checker().serve((host, port));
}

#[derive(Clone, Debug)]
struct DepthTracker {
    depths: Arc<BTreeMap<usize, Arc<AtomicU64>>>,
    consistency: ConsistencySetup,
    controllers: usize,
    function: String,
}

impl DepthTracker {
    fn new(
        max: usize,
        consistency: ConsistencySetup,
        controllers: usize,
        function: String,
    ) -> Self {
        let mut depths = BTreeMap::new();
        for i in 0..=max {
            depths.insert(i, Arc::new(AtomicU64::new(0)));
        }
        Self {
            depths: Arc::new(depths),
            consistency,
            controllers,
            function,
        }
    }

    fn to_csv(&self, path: &Path) {
        let mut writer = csv::Writer::from_path(path).unwrap();
        writer
            .write_record(["depth", "count", "consistency", "controllers", "function"])
            .unwrap();
        for (d, c) in &*self.depths {
            writer
                .write_record([
                    d.to_string(),
                    c.load(std::sync::atomic::Ordering::Relaxed).to_string(),
                    self.consistency.to_string(),
                    self.controllers.to_string(),
                    self.function.to_owned(),
                ])
                .unwrap();
        }
        writer.flush().unwrap()
    }
}

impl<M> stateright::CheckerTerminalVisitor<M> for DepthTracker
where
    M: Model,
{
    fn visit(&self, _model: &M, path: &[NonZeroU64]) {
        let len = path.len();
        self.depths
            .get(&len)
            .unwrap()
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}
