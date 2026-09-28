import 'package:editor_ui/features/create_project/model/project_creator.dart';
import 'package:flutter/material.dart';

Future<ProjectCreationSuccess?> showCreateProjectDialog(
  BuildContext context, {
  required ProjectCreator creator,
}) {
  return showDialog<ProjectCreationSuccess>(
    context: context,
    builder: (context) => _CreateProjectDialog(creator: creator),
  );
}

class _CreateProjectDialog extends StatefulWidget {
  const _CreateProjectDialog({required this.creator});

  final ProjectCreator creator;

  @override
  State<_CreateProjectDialog> createState() => _CreateProjectDialogState();
}

class _CreateProjectDialogState extends State<_CreateProjectDialog> {
  final _directoryController = TextEditingController();
  String? _errorMessage;
  bool _isCreating = false;

  @override
  void dispose() {
    _directoryController.dispose();
    super.dispose();
  }

  Future<void> _createProject() async {
    final directory = _directoryController.text.trim();
    if (directory.isEmpty) {
      setState(() => _errorMessage = 'Enter a directory for the new project.');
      return;
    }

    setState(() {
      _errorMessage = null;
      _isCreating = true;
    });
    final result = await widget.creator.create(directory);
    if (!mounted) {
      return;
    }

    switch (result) {
      case ProjectCreationSuccess():
        Navigator.of(context).pop(result);
      case ProjectCreationFailure(:final message, :final diagnostic):
        setState(() {
          _isCreating = false;
          _errorMessage = diagnostic == null
              ? message
              : '$message\n$diagnostic';
        });
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Create project'),
      content: SizedBox(
        width: 440,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('Enter the folder path for the new Cargo project.'),
            const SizedBox(height: 16),
            TextField(
              controller: _directoryController,
              autofocus: true,
              enabled: !_isCreating,
              decoration: InputDecoration(
                labelText: 'Project directory',
                hintText: r'C:\Projects\my_game',
                errorText: _errorMessage,
                border: const OutlineInputBorder(),
              ),
              onSubmitted: (_) => _isCreating ? null : _createProject(),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _isCreating ? null : () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton.icon(
          onPressed: _isCreating ? null : _createProject,
          icon: _isCreating
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.add),
          label: Text(_isCreating ? 'Creating…' : 'Create'),
        ),
      ],
    );
  }
}
