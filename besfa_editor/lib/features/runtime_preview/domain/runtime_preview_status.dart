/// Editor-facing preview runtime status.
enum RuntimePreviewStatus {
  /// Preview runtime is not running.
  stopped,

  /// Preview runtime is running and ready.
  running,

  /// Preview runtime failed to start, stop, or report state.
  failed;

  /// User-facing status label.
  String get label {
    return switch (this) {
      RuntimePreviewStatus.stopped => 'Stopped',
      RuntimePreviewStatus.running => 'Running',
      RuntimePreviewStatus.failed => 'Failed',
    };
  }
}
