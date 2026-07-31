import 'package:flutter/material.dart';

class ProjectHubPage extends StatelessWidget {
  const ProjectHubPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: Column(
          children: [
            const _AppHeader(),
            Expanded(
              child: Center(
                child: SingleChildScrollView(
                  padding: const EdgeInsets.all(32),
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 920),
                    child: const _ProjectHubContent(),
                  ),
                ),
              ),
            ),
            const _Footer(),
          ],
        ),
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
  const _ProjectHubContent();

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
            const children = [
              Expanded(
                child: _ProjectActionCard(
                  icon: Icons.add_box_outlined,
                  title: 'Create project',
                  description: 'Start a new game from a Besfa template.',
                  action: 'New project',
                  emphasized: true,
                ),
              ),
              SizedBox(width: 16, height: 16),
              Expanded(
                child: _ProjectActionCard(
                  icon: Icons.folder_open_outlined,
                  title: 'Open project',
                  description: 'Open an existing Besfa project folder.',
                  action: 'Open folder',
                ),
              ),
            ];

            return constraints.maxWidth < 680
                ? const Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: children,
                  )
                : const Row(children: children);
          },
        ),
        const SizedBox(height: 48),
        Text('Recent projects', style: Theme.of(context).textTheme.titleLarge),
        const SizedBox(height: 16),
        const _EmptyRecentProjects(),
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
    this.emphasized = false,
  });

  final IconData icon;
  final String title;
  final String description;
  final String action;
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
                    onPressed: () {},
                    icon: const Icon(Icons.add),
                    label: Text(action),
                  )
                : OutlinedButton.icon(
                    onPressed: () {},
                    icon: const Icon(Icons.folder_open_outlined),
                    label: Text(action),
                  ),
          ],
        ),
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
