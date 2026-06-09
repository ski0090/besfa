/// Editor-facing preview runtime status.
enum RuntimePreviewStatus {
  /// Preview runtime is not running.
  stopped,

  /// Preview runtime is starting and waiting for IPC readiness.
  starting,

  /// Preview runtime is running and ready.
  running,

  /// Preview runtime failed to start, stop, or report state.
  failed;

  /// User-facing status label.
  String get label {
    return switch (this) {
      RuntimePreviewStatus.stopped => 'Offline',
      RuntimePreviewStatus.starting => 'Starting',
      RuntimePreviewStatus.running => 'Scene Ready',
      RuntimePreviewStatus.failed => 'Failed',
    };
  }
}

/// Editor-facing scene playback state inside the running preview runtime.
enum RuntimeScenePlaybackState {
  /// Runtime game time is paused and the scene is in edit mode.
  stopped,

  /// Runtime game time is advancing.
  playing;

  /// Human-readable label for editor controls.
  String get label {
    return switch (this) {
      RuntimeScenePlaybackState.stopped => 'Stopped',
      RuntimeScenePlaybackState.playing => 'Playing',
    };
  }
}
