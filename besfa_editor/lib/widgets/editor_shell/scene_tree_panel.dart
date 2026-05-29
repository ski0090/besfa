import 'package:besfa_editor/shared/ui/panel_title.dart';
import 'package:flutter/material.dart';

class SceneTreePanel extends StatelessWidget {
  const SceneTreePanel({super.key});

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.all(12),
      children: const [
        PanelTitle('Scene'),
        _TreeItem(icon: Icons.public, label: 'World'),
        _TreeItem(icon: Icons.videocam, label: 'Camera3d', indent: 16),
        _TreeItem(icon: Icons.light_mode, label: 'Key Light', indent: 16),
        _TreeItem(icon: Icons.grid_on, label: 'Ground', indent: 16),
      ],
    );
  }
}

class _TreeItem extends StatelessWidget {
  const _TreeItem({required this.icon, required this.label, this.indent = 0});

  final IconData icon;
  final String label;
  final double indent;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(left: indent),
      child: ListTile(
        dense: true,
        leading: Icon(icon, size: 18),
        title: Text(label),
        contentPadding: EdgeInsets.zero,
      ),
    );
  }
}
