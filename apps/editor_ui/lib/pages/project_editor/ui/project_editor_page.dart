import 'package:flutter/material.dart';

import 'package:editor_ui/entities/project/model/project.dart';

class ProjectEditorPage extends StatelessWidget {
  const ProjectEditorPage({super.key, required this.project});

  final Project project;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        automaticallyImplyLeading: false,
        actions: [
          TextButton.icon(
            onPressed: () => Navigator.of(context).pop(),
            icon: const Icon(Icons.close),
            label: const Text('Close project'),
          ),
          const SizedBox(width: 12),
        ],
      ),
      body: const Center(
        child: Text('Viewport', style: TextStyle(color: Color(0xFF7E8795))),
      ),
    );
  }
}
