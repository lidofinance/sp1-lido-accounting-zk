use chrono::Utc;
use cron::Schedule;
use sp1_lido_accounting_scripts::utils::read_env;
use std::{env, sync::Arc, thread};
use tokio::time::Duration;
use tracing::Span;

use crate::common::{run_submit, AppState, Error};

async fn scheduler_loop(state: Arc<AppState>, schedule: Schedule, timezone: chrono_tz::Tz) {
    let upcoming = schedule.upcoming(timezone);

    for next in upcoming {
        let now = Utc::now().with_timezone(&timezone);
        let duration = next - now;
        let sleep_duration = duration.to_std().unwrap_or(Duration::from_secs(0));
        tracing::info!(
            "Next run at {} ({} seconds)",
            next,
            sleep_duration.as_secs()
        );

        tokio::time::sleep(sleep_duration).await;
        submit_report(Arc::clone(&state)).await;
    }
}

async fn submit_report(state: Arc<AppState>) {
    state
        .script_runtime
        .metrics
        .metadata
        .run_report_counter
        .with_label_values(&["scheduler"])
        .inc();
    let result = run_submit(&state, None, None).await;
    match result {
        Ok(tx_hash) => tracing::info!("Successfully submitted report, txhash: {}", tx_hash),
        Err(e) => match e {
            Error::AlreadyRunning => tracing::warn!("Already running - skipping scheduled run"),
            Error::SubmitError(underlying) => tracing::error!("Failed to run: {underlying:?}"),
        },
    }
}

pub fn launch(state: Arc<AppState>, parent_span: Span) -> Option<thread::JoinHandle<()>> {
    let enabled = read_env("INTERNAL_SCHEDULER", false);

    if !enabled {
        tracing::info!("Scheduler disabled");
        return None;
    }

    tracing::debug!("Scheduler enabled, reading schedule expression");
    // Read cron expression
    let schedule = env::var("INTERNAL_SCHEDULER_CRON")
        .unwrap_or_else(|e| panic!("Failed to read INTERNAL_SCHEDULER_CRON: {e:?}"))
        .parse()
        .unwrap_or_else(|e| panic!("Failed to parse INTERNAL_SCHEDULER_CRON: {e:?}"));

    let tz: chrono_tz::Tz = env::var("INTERNAL_SCHEDULER_TZ")
        .unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to read INTERNAL_SCHEDULER_TZ env var - assuming UTC. Error: {e:?}"
            );
            "UTC".to_owned()
        })
        .parse()
        .unwrap_or_else(|e| panic!("Failed to parse INTERNAL_SCHEDULER_TZ: {e:?}"));

    tracing::info!(
        "Scheduler enabled. Using timezone {} and schedule: {}",
        tz,
        schedule
    );

    // Spawn scheduler thread
    let join_handle = thread::Builder::new()
        .name("scheduler".into())
        .spawn(move || {
            let _enter = parent_span.enter();
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(scheduler_loop(state, schedule, tz));
        })
        .unwrap();
    Some(join_handle)
}
