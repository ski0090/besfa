import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/entities/project/model/recent_projects.dart';
import 'package:editor_ui/features/create_project/model/project_creator.dart';
import 'package:editor_ui/features/create_project/ui/create_project_dialog.dart';
import 'package:editor_ui/features/delete_project/model/project_deleter.dart';
import 'package:editor_ui/features/open_project/model/project_loader.dart';

class ProjectHubPage extends StatefulWidget {
  const ProjectHubPage({
    super.key,
    this.projectCreator = const BesfaCliProjectCreator(),
    required this.recentProjects,
    this.deleteProject = moveToRecycleBin,
  });

  final ProjectCreator projectCreator;
  final RecentProjects recentProjects;
  final Future<bool> Function(String directory) deleteProject;

  /// Route registered by the app that shows the editor for a [Project].
  static const editorRoute = '/editor';

  @override
  State<ProjectHubPage> createState() => _ProjectHubPageState();
}

class _ProjectHubPageState extends State<ProjectHubPage> {
  late var _recent = widget.recentProjects.load();

  void _openEditor(Project project) {
    setState(() => _recent = widget.recentProjects.add(project));
    Navigator.of(
      context,
    ).pushNamed(ProjectHubPage.editorRoute, arguments: project);
  }

  void _showMessage(String message) {
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text(message)));
  }

  Future<void> _showCreateProject() async {
    final result = await showCreateProjectDialog(
      context,
      creator: widget.projectCreator,
    );
    if (!mounted || result == null) {
      return;
    }

    _openEditor(Project(result.projectPath));
  }

  Future<void> _showOpenProject() async {
    final directory = await getDirectoryPath(confirmButtonText: 'Open');
    if (!mounted || directory == null) {
      return;
    }

    final project = loadProject(directory);
    if (project == null) {
      _showMessage("'$directory' has no Cargo.toml.");
      return;
    }
    _openEditor(project);
  }

  void _openRecent(Project project) {
    if (loadProject(project.path) == null) {
      setState(() => _recent = widget.recentProjects.remove(project));
      _showMessage(
        "'${project.path}' is no longer a Cargo project and was removed.",
      );
      return;
    }
    _openEditor(project);
  }

  Future<void> _deleteRecent(Project project) async {
    // Never delete a folder that is not (or no longer) a Cargo project.
    if (loadProject(project.path) == null) {
      setState(() => _recent = widget.recentProjects.remove(project));
      _showMessage(
        "'${project.path}' is no longer a Cargo project and was removed.",
      );
      return;
    }

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text("Delete '${project.name}'?"),
        content: Text("'${project.path}' will be moved to the Recycle Bin."),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Delete'),
          ),
        ],
      ),
    );
    if (!mounted || confirmed != true) {
      return;
    }

    if (!await widget.deleteProject(project.path)) {
      if (mounted) _showMessage("Could not delete '${project.path}'.");
      return;
    }
    final recent = widget.recentProjects.remove(project);
    if (mounted) setState(() => _recent = recent);
  }

  void _revealRecent(Project project) {
    if (!Directory(project.path).existsSync()) {
      _showMessage("'${project.path}' no longer exists.");
      return;
    }
    // Explorer needs backslashes; with '/' it silently opens Documents.
    Process.start('explorer', [
      project.path.replaceAll('/', r'\'),
    ], mode: ProcessStartMode.detached);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Column(
        children: [
          const _AppHeader(),
          Expanded(
            child: Center(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(32),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 920),
                  child: _ProjectHubContent(
                    recent: _recent,
                    onCreateProject: _showCreateProject,
                    onOpenProject: _showOpenProject,
                    onOpenRecent: _openRecent,
                    onRevealRecent: _revealRecent,
                    onDeleteRecent: _deleteRecent,
                  ),
                ),
              ),
            ),
          ),
          const _Footer(),
        ],
      ),
    );
  }
}

class _AppHeader extends StatelessWidget {
  const _AppHeader();

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 64,
      padding: const EdgeInsets.symmetric(horizontal: 28),
      decoration: BoxDecoration(
        border: Border(
          bottom: BorderSide(color: Colors.white.withValues(alpha: .08)),
        ),
      ),
      child: const Row(
        children: [
          _BesfaMark(),
          SizedBox(width: 12),
          Text(
            'Besfa',
            style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
          ),
          Spacer(),
          Text('Editor Preview', style: TextStyle(color: Color(0xFF9DA6B5))),
        ],
      ),
    );
  }
}

class _BesfaMark extends StatelessWidget {
  const _BesfaMark();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 28,
      height: 28,
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.primary,
        borderRadius: BorderRadius.circular(8),
      ),
      child: const Icon(Icons.change_history_rounded, size: 19),
    );
  }
}

class _ProjectHubContent extends StatelessWidget {
  const _ProjectHubContent({
    required this.recent,
    required this.onCreateProject,
    required this.onOpenProject,
    required this.onOpenRecent,
    required this.onRevealRecent,
    required this.onDeleteRecent,
  });

  final List<Project> recent;
  final VoidCallback onCreateProject;
  final VoidCallback onOpenProject;
  final ValueChanged<Project> onOpenRecent;
  final ValueChanged<Project> onRevealRecent;
  final ValueChanged<Project> onDeleteRecent;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Start creating', style: Theme.of(context).textTheme.displaySmall),
        const SizedBox(height: 10),
        Text(
          'Create a new Besfa project or open one you already have.',
          style: Theme.of(
            context,
          ).textTheme.bodyLarge?.copyWith(color: const Color(0xFFB3BBC8)),
        ),
        const SizedBox(height: 32),
        LayoutBuilder(
          builder: (context, constraints) {
            final children = [
              Expanded(
                child: _ProjectActionCard(
                  icon: Icons.add_box_outlined,
                  title: 'Create project',
                  description: 'Start a new binary Rust game project.',
                  action: 'New project',
                  emphasized: true,
                  onPressed: onCreateProject,
                ),
              ),
              SizedBox(width: 16, height: 16),
              Expanded(
                child: _ProjectActionCard(
                  icon: Icons.folder_open_outlined,
                  title: 'Open project',
                  description: 'Open an existing Besfa project folder.',
                  action: 'Open folder',
                  onPressed: onOpenProject,
                ),
              ),
            ];

            return constraints.maxWidth < 680
                ? Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: children,
                  )
                : Row(children: children);
          },
        ),
        const SizedBox(height: 48),
        Text('Recent projects', style: Theme.of(context).textTheme.titleLarge),
        const SizedBox(height: 16),
        recent.isEmpty
            ? const _EmptyRecentProjects()
            : _RecentProjectList(
                recent,
                onOpen: onOpenRecent,
                onReveal: onRevealRecent,
                onDelete: onDeleteRecent,
              ),
      ],
    );
  }
}

class _ProjectActionCard extends StatelessWidget {
  const _ProjectActionCard({
    required this.icon,
    required this.title,
    required this.description,
    required this.action,
    required this.onPressed,
    this.emphasized = false,
  });

  final IconData icon;
  final String title;
  final String description;
  final String action;
  final VoidCallback onPressed;
  final bool emphasized;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(icon, color: colors.primary, size: 30),
            const SizedBox(height: 36),
            Text(title, style: Theme.of(context).textTheme.titleLarge),
            const SizedBox(height: 8),
            Text(description, style: const TextStyle(color: Color(0xFFB3BBC8))),
            const SizedBox(height: 24),
            emphasized
                ? FilledButton.icon(
                    onPressed: onPressed,
                    icon: const Icon(Icons.add),
                    label: Text(action),
                  )
                : OutlinedButton.icon(
                    onPressed: onPressed,
                    icon: const Icon(Icons.folder_open_outlined),
                    label: Text(action),
                  ),
          ],
        ),
      ),
    );
  }
}

class _RecentProjectList extends StatelessWidget {
  const _RecentProjectList(
    this.projects, {
    required this.onOpen,
    required this.onReveal,
    required this.onDelete,
  });

  final List<Project> projects;
  final ValueChanged<Project> onOpen;
  final ValueChanged<Project> onReveal;
  final ValueChanged<Project> onDelete;

  @override
  Widget build(BuildContext context) {
    return Card(
      clipBehavior: Clip.antiAlias,
      child: Column(
        children: [
          for (final project in projects)
            ListTile(
              leading: const Icon(Icons.folder_outlined),
              title: Text(project.name),
              subtitle: Text(
                project.path,
                style: const TextStyle(color: Color(0xFF9DA6B5)),
              ),
              trailing: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  IconButton(
                    tooltip: 'Show in Explorer',
                    icon: const Icon(Icons.folder_open_outlined),
                    onPressed: () => onReveal(project),
                  ),
                  IconButton(
                    tooltip: 'Delete project',
                    icon: const Icon(Icons.delete_outline),
                    onPressed: () => onDelete(project),
                  ),
                ],
              ),
              onTap: () => onOpen(project),
            ),
        ],
      ),
    );
  }
}

class _EmptyRecentProjects extends StatelessWidget {
  const _EmptyRecentProjects();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(vertical: 46, horizontal: 24),
      decoration: BoxDecoration(
        border: Border.all(color: Colors.white.withValues(alpha: .1)),
        borderRadius: BorderRadius.circular(12),
      ),
      child: const Column(
        children: [
          Icon(Icons.history_rounded, color: Color(0xFF7E8795), size: 28),
          SizedBox(height: 12),
          Text('No recent projects'),
          SizedBox(height: 4),
          Text(
            'Projects you open will appear here.',
            style: TextStyle(color: Color(0xFF9DA6B5)),
          ),
        ],
      ),
    );
  }
}

class _Footer extends StatelessWidget {
  const _Footer();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 28, vertical: 14),
      child: Row(
        children: [
          const Text(
            'Besfa Editor',
            style: TextStyle(color: Color(0xFF7E8795)),
          ),
          const Spacer(),
          Text(
            '0.1.0-dev',
            style: Theme.of(
              context,
            ).textTheme.bodySmall?.copyWith(color: const Color(0xFF7E8795)),
          ),
        ],
      ),
    );
  }
}
