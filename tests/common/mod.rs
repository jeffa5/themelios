use model_checked_orchestration::model::OrchestrationModelCfg;
use model_checked_orchestration::report::Reporter;
use stateright::Checker;
use stateright::HasDiscoveries;
use stateright::Model;
use stateright::UniformChooser;
use std::collections::BTreeMap;

use model_checked_orchestration::resources::Meta;

// Check that the annotations on resource `a` are all set on resource `b`.
pub fn annotations_subset<T, U>(a: &T, b: &U) -> bool
where
    T: Meta,
    U: Meta,
{
    subset(&a.metadata().annotations, &b.metadata().annotations)
}

fn subset(m1: &BTreeMap<String, String>, m2: &BTreeMap<String, String>) -> bool {
    m1.iter().all(|(k, v)| m2.get(k).map_or(false, |w| v == w))
}

pub fn run(model: OrchestrationModelCfg, fn_name: &str) {
    println!("Running test {:?}", fn_name);
    if let Ok(explore_test) = std::env::var("MCO_EXPLORE_TEST") {
        if fn_name.ends_with(&explore_test) {
            explore(model);
            return;
        } else {
            // skip others
            return;
        }
    }
    check(model)
}

fn check(model: OrchestrationModelCfg) {
    println!("Checking model");
    let am = model.into_abstract_model();
    let mut reporter = Reporter::new(&am);
    let checker = am
        .checker()
        .threads(num_cpus::get())
        .finish_when(HasDiscoveries::AnyFailures);
    let check_mode = std::env::var("MCO_CHECK_MODE").unwrap_or_else(|_| "bfs".to_owned());
    // skip clippy bit here for clarity
    #[allow(clippy::wildcard_in_or_patterns)]
    match check_mode.as_str() {
        "simulation" => checker
            .spawn_simulation(0, UniformChooser)
            .report(&mut reporter)
            .assert_properties(),
        "dfs" => checker
            .spawn_bfs()
            .report(&mut reporter)
            .assert_properties(),
        "bfs" | _ => checker
            .spawn_bfs()
            .report(&mut reporter)
            .assert_properties(),
    }
}

fn explore(model: OrchestrationModelCfg) {
    let host = "127.0.0.1";
    let port = 8080;
    println!("Exploring model, served on http://{}:{}", host, port);
    let am = model.into_abstract_model();
    am.checker().serve((host, port));
}
