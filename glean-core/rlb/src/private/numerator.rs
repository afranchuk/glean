// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use inherent::inherent;
use std::sync::Arc;

use glean_core::metrics::MetricType;
use glean_core::ErrorType;

use crate::dispatcher;

// We need to wrap the glean-core type: otherwise if we try to implement
// the trait for the metric in `glean_core::metrics` we hit error[E0117]:
// only traits defined in the current crate can be implemented for arbitrary
// types.

/// Developer-facing API for recording rate metrics with external denominators.
///
/// Instances of this class type are automatically generated by the parsers
/// at build time, allowing developers to record values that were previously
/// registered in the metrics.yaml file.
#[derive(Clone)]
pub struct NumeratorMetric(pub(crate) Arc<glean_core::metrics::RateMetric>);

impl NumeratorMetric {
    /// The public constructor used by automatically generated metrics.
    pub fn new(meta: glean_core::CommonMetricData) -> Self {
        Self(Arc::new(glean_core::metrics::RateMetric::new(meta)))
    }
}

#[inherent(pub)]
impl glean_core::traits::Numerator for NumeratorMetric {
    fn add_to_numerator(&self, amount: i32) {
        let metric = Arc::clone(&self.0);
        dispatcher::launch(move || {
            crate::with_glean(|glean| metric.add_to_numerator(glean, amount))
        });
    }

    fn test_get_value<'a, S: Into<Option<&'a str>>>(&self, ping_name: S) -> Option<(i32, i32)> {
        crate::block_on_dispatcher();

        let queried_ping_name = ping_name
            .into()
            .unwrap_or_else(|| &self.0.meta().send_in_pings[0]);

        crate::with_glean(|glean| self.0.test_get_value(glean, queried_ping_name))
    }

    fn test_get_num_recorded_errors<'a, S: Into<Option<&'a str>>>(
        &self,
        error: ErrorType,
        ping_name: S,
    ) -> i32 {
        crate::block_on_dispatcher();

        crate::with_glean_mut(|glean| {
            glean_core::test_get_num_recorded_errors(glean, self.0.meta(), error, ping_name.into())
                .unwrap_or(0)
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::common_test::{lock_test, new_glean};
    use crate::CommonMetricData;

    #[test]
    fn numerator_smoke() {
        let _lock = lock_test();
        let _t = new_glean(None, true);

        let metric: NumeratorMetric = NumeratorMetric::new(CommonMetricData {
            name: "rate".into(),
            category: "test".into(),
            send_in_pings: vec!["test1".into()],
            ..Default::default()
        });

        // Adding 0 doesn't error.
        metric.add_to_numerator(0);

        assert_eq!(
            metric.test_get_num_recorded_errors(ErrorType::InvalidValue, None),
            0
        );

        // Adding a negative value errors.
        metric.add_to_numerator(-1);

        assert_eq!(
            metric.test_get_num_recorded_errors(ErrorType::InvalidValue, None),
            1
        );

        // Getting the value returns 0s if that's all we have.
        assert_eq!(metric.test_get_value(None), Some((0, 0)));

        // And normal values of course work.
        metric.add_to_numerator(22);

        assert_eq!(metric.test_get_value(None), Some((22, 0)));
    }
}