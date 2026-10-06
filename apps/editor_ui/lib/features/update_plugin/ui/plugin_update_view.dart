import 'package:flutter/material.dart';

/// The project's `besfa_editor_plugin` revision, against the one the
/// Bevy prebuild has, with a way to update it. Hidden for projects whose
/// lock does not pin the plugin to a git revision.
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
    final project = this.project;
    if (project == null) {
      return const SizedBox.shrink();
    }
    final outdated = latest != null && latest != project;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          // Different, not necessarily older: a push made while the editor
          // runs leaves the project ahead of the prebuild.
          outdated
              ? 'Plugin ${_short(project)}, prebuild ${_short(latest!)}'
              : 'Plugin ${_short(project)}',
          style: TextStyle(
            fontSize: 12,
            color: outdated ? const Color(0xFFE8B86D) : const Color(0xFF7E8795),
          ),
        ),
        if (outdated)
          TextButton(
            onPressed: busy ? null : onUpdate,
            child: const Text('Update'),
          )
        else
          IconButton(
            tooltip: 'Update besfa_editor_plugin to its latest commit',
            iconSize: 16,
            onPressed: busy ? null : onUpdate,
            icon: const Icon(Icons.refresh),
          ),
      ],
    );
  }
}

String _short(String revision) =>
    revision.length > 7 ? revision.substring(0, 7) : revision;
