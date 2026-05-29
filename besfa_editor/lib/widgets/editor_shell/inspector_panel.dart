import 'package:besfa_editor/shared/ui/panel_title.dart';
import 'package:flutter/material.dart';

class InspectorPanel extends StatefulWidget {
  const InspectorPanel({super.key});

  @override
  State<InspectorPanel> createState() => _InspectorPanelState();
}

class _InspectorPanelState extends State<InspectorPanel> {
  final TextEditingController _name = TextEditingController(text: 'Camera3d');

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.all(12),
      children: [
        const PanelTitle('Inspector'),
        TextField(
          controller: _name,
          decoration: const InputDecoration(
            labelText: 'Name',
            border: OutlineInputBorder(),
          ),
        ),
        const SizedBox(height: 12),
        const _PropertyRow(label: 'Transform', value: '0, 4, 8'),
        const _PropertyRow(label: 'Projection', value: 'Perspective'),
        const _PropertyRow(label: 'Runtime', value: 'Bevy 0.18'),
      ],
    );
  }
}

class _PropertyRow extends StatelessWidget {
  const _PropertyRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(
        children: [
          SizedBox(
            width: 92,
            child: Text(label, style: Theme.of(context).textTheme.labelMedium),
          ),
          Expanded(child: Text(value)),
        ],
      ),
    );
  }
}
