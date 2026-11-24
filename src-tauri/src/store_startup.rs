use windows::core::HSTRING;
use windows::ApplicationModel::{StartupTask, StartupTaskState};

fn startup_task() -> windows::core::Result<StartupTask> {
    StartupTask::GetAsync(&HSTRING::from("SessionMeterStartup"))?.get()
}

pub fn is_enabled() -> Result<bool, String> {
    let state = startup_task()
        .and_then(|task| task.State())
        .map_err(|error| error.to_string())?;
    Ok(matches!(
        state,
        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy
    ))
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let task = startup_task().map_err(|error| error.to_string())?;
    if enabled {
        let state = task
            .RequestEnableAsync()
            .and_then(|request| request.get())
            .map_err(|error| error.to_string())?;
        if !matches!(
            state,
            StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy
        ) {
            return Err("Windows startup settings did not enable the task".into());
        }
    } else {
        task.Disable().map_err(|error| error.to_string())?;
        if is_enabled()? {
            return Err("Windows startup settings did not disable the task".into());
        }
    }
    Ok(())
}
