use crate::args::SimConfig;
use crate::bugbase::{BugBase, BugRecord, BugRecordKind};
use crate::comparator::ComparisonMismatch;
use crate::plan;
use crate::runner::RunError;
use crate::shrinker::ShrinkResult;

use super::{FailureContext, ModeError};

pub(super) fn dump_plan(path: &std::path::Path, plan: &plan::Plan) -> Result<(), String> {
    let content = serde_json::to_string_pretty(plan)
        .map_err(|err| format!("failed to serialize plan: {err}"))?;
    std::fs::write(path, content).map_err(|err| format!("failed to write plan file: {err}"))?;
    Ok(())
}

pub(super) fn report_failure(config: &SimConfig, plan: &plan::Plan, context: FailureContext) -> ! {
    if let Some(path) = config.dump_plan_on_failure.as_deref() {
        emit_dump(path, plan);
    }
    emit_bugbase(config, plan, &context);
    emit_shrink_report(context.shrink_result.as_ref());
    report_mode_error(context.error);
    std::process::exit(1);
}

fn emit_dump(path: &std::path::Path, plan: &plan::Plan) {
    if let Err(dump_err) = dump_plan(path, plan) {
        eprintln!("failed to dump plan to {}: {dump_err}", path.display());
    } else {
        eprintln!(
            "dumped failing plan to {} (replay with --plan {})",
            path.display(),
            path.display()
        );
    }
}

fn emit_bugbase(config: &SimConfig, plan: &plan::Plan, context: &FailureContext) {
    match maybe_write_bugbase(config, plan, context) {
        Ok(Some(path)) => eprintln!("wrote bugbase entry to {}", path.display()),
        Ok(None) => {}
        Err(err) => eprintln!("failed to write bugbase entry: {err}"),
    }
}

fn emit_shrink_report(shrink_result: Option<&ShrinkResult>) {
    if let Some(shrink_result) = shrink_result {
        let shrink_report = &shrink_result.report;
        eprintln!(
            "shrunk failing plan from {} to {} steps in {} rounds ({} attempts)",
            shrink_report.original_steps,
            shrink_report.shrunk_steps,
            shrink_report.rounds,
            shrink_report.attempts
        );
    }
}

fn report_mode_error(err: ModeError) {
    match err {
        ModeError::PlanRun { label, error } => report_run_error(label, &error),
        ModeError::Compare(mismatch) => {
            eprintln!(
                "comparison failed at step {}: {}",
                mismatch.step, mismatch.reason
            );
        }
        ModeError::Setup { label, reason } => {
            eprintln!("{label} setup failed: {reason}");
        }
    }
}

fn report_run_error(label: &'static str, error: &RunError) {
    eprintln!(
        "{label} run failed at step {} (task {}): {}",
        error.step, error.task, error.reason
    );
}

fn maybe_write_bugbase(
    config: &SimConfig,
    plan: &plan::Plan,
    context: &FailureContext,
) -> Result<Option<std::path::PathBuf>, String> {
    let bugbase_dir = match &config.bugbase_dir {
        Some(dir) => dir.clone(),
        None => return Ok(None),
    };

    let record = build_bug_record(&context.error);
    let shrunk_plan = context.shrink_result.as_ref().and_then(|result| {
        (result.report.original_steps != result.report.shrunk_steps).then_some(&result.plan)
    });

    let bugbase = BugBase::new(bugbase_dir);
    let path = bugbase.store_failure(config, plan, shrunk_plan, &record)?;
    Ok(Some(path))
}

fn build_bug_record(error: &ModeError) -> BugRecord {
    match error {
        ModeError::PlanRun { label, error } => BugRecord {
            kind: BugRecordKind::PlanRun,
            label: (*label).to_string(),
            error: error.reason.clone(),
            step: Some(error.step),
            task: Some(error.task),
            action: Some(error.action.clone()),
            comparison_step: None,
        },
        ModeError::Compare(ComparisonMismatch { step, reason }) => BugRecord {
            kind: BugRecordKind::Compare,
            label: "compare".to_string(),
            error: reason.clone(),
            step: None,
            task: None,
            action: None,
            comparison_step: Some(*step),
        },
        ModeError::Setup { label, reason } => BugRecord {
            kind: BugRecordKind::Setup,
            label: (*label).to_string(),
            error: reason.clone(),
            step: None,
            task: None,
            action: None,
            comparison_step: None,
        },
    }
}
