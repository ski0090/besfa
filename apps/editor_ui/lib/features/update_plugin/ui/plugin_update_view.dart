import 'package:flutter/material.dart';

/// Whether the project's `besfa_editor_plugin` matches the one the Bevy
/// prebuild has, as a status icon that updates the plugin when pressed.
/// Hidden for projects whose lock does not pin the plugin to a git revision.
class PluginUpdateView extends StatelessWidget {
  const PluginUpdateView({
    super.key,
    required this.project,
    required this.latest,
    required this.busy,
    required this.onUpdate,
  });

  /// Revision in the project's Cargo.lock.
  final String? project;

  /// Revision the prebuild was built with; null when there is no prebuild.
  final String? latest;

  final bool busy;
  final VoidCallback onUpdate;

  @override
  Widget build(BuildContext context) {
    if (project == null) {
      return const SizedBox.shrink();
    }
    // Different, not necessarily older: a push made while the editor runs
    // leaves the project ahead of the prebuild.
    final differs = latest != null && latest != project;
    return IconButton(
      tooltip: differs
          ? 'The plugin differs from the Bevy prebuild. '
                'Click to update it to the latest commit.'
          : 'The plugin matches the Bevy prebuild. '
                'Click to update it to the latest commit.',
      iconSize: 18,
      onPressed: busy ? null : onUpdate,
      icon: Icon(
        differs ? Icons.warning_amber_rounded : Icons.check_circle_outline,
        color: differs ? const Color(0xFFE8B86D) : const Color(0xFF7EC98F),
      ),
    );
  }
}
