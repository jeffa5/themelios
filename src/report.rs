use std::collections::BTreeMap;

use stateright::{Expectation, Model};

#[derive(Debug, Default)]
pub struct Reporter {
    last_total: usize,
    last_unique: usize,
    properties: BTreeMap<&'static str, Expectation>,
}

impl Reporter {
    /// Create a new reporter.
    pub fn new<M: Model>(model: &M) -> Self {
        let properties = model
            .properties()
            .iter()
            .map(|p| (p.name, p.expectation.clone()))
            .collect();
        Self {
            last_total: 0,
            last_unique: 0,
            properties,
        }
    }
}

impl<M> stateright::report::Reporter<M> for Reporter
where
    M: Model,
{
    fn report_checking(&mut self, data: stateright::report::ReportData) {
        let new_total = data.total_states - self.last_total;
        let total_rate = (data.total_states as f64 / data.duration.as_secs_f64()).round();
        let new_unique = data.unique_states - self.last_unique;
        let unique_rate = (data.unique_states as f64 / data.duration.as_secs_f64()).round();
        let status = if data.done { "Done    " } else { "Checking" };
        let depth = data.max_depth;
        println!(
            "{} states={: >8} (+{: <8} {: >8.0}/s), unique={: >8} (+{: <8} {: >8}/s), max_depth={: >4}, duration={:?}",
            status,
            data.total_states,
            new_total,
            total_rate,
            data.unique_states,
            new_unique,
            unique_rate,
            depth,
            data.duration
        );

        self.last_total = data.total_states;
        self.last_unique = data.unique_states;
    }

    fn report_discoveries(
        &mut self,
        discoveries: std::collections::BTreeMap<
            &'static str,
            stateright::report::ReportDiscovery<M>,
        >,
    ) where
        <M as Model>::Action: std::fmt::Debug,
        <M as Model>::State: std::fmt::Debug + std::hash::Hash,
    {
        let (success, failure): (Vec<_>, Vec<_>) =
            self.properties.iter().partition(|(name, expectation)| {
                property_holds(expectation, discoveries.get(*name).is_some())
            });

        for (name, expectation) in &self.properties {
            let status = if property_holds(expectation, discoveries.get(name).is_some()) {
                "OK"
            } else {
                "FAILED"
            };
            println!("Property {:?} {:?} {}", expectation, name, status);
            if let Some(discovery) = discoveries.get(name) {
                print!("{}, {}", discovery.classification, discovery.path,);
                println!(
                    "To explore this path try re-running with `explore {}`",
                    discovery.path.encode()
                );
            }
        }

        println!(
            "Properties checked. {} succeeded, {} failed",
            success.len(),
            failure.len()
        );
    }
}

fn property_holds(expectation: &Expectation, discovery: bool) -> bool {
    match (expectation, discovery) {
        // counter-example
        (Expectation::Always, true) => false,
        // no counter-example
        (Expectation::Always, false) => true,
        // counter-example
        (Expectation::Eventually, true) => false,
        // no counter-example
        (Expectation::Eventually, false) => true,
        // example
        (Expectation::Sometimes, true) => true,
        // no example
        (Expectation::Sometimes, false) => false,
    }
}
