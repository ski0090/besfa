import 'dart:convert';
import 'dart:io';

abstract interface class ProjectCreator {
  Future<ProjectCreationResult> create(String directory);
}

class BesfaCliProjectCreator implements ProjectCreator {
  const BesfaCliProjectCreator({this.executable = 'besfa'});

  final String executable;

  @override
  Future<ProjectCreationResult> create(String directory) async {
    try {
      final result = await Process.run(executable, [
        'new',
        directory,
        '--output',
        'json',
      ]);
      final output = _decodeOutput(result.stdout);

      if (result.exitCode == 0 && output['status'] == 'success') {
        final projectPath = output['project_path'];
        if (projectPath is String) {
          return ProjectCreationSuccess(projectPath);
        }
      }

      final message = output['message'];
      return ProjectCreationFailure(
        message is String ? message : 'Failed to create the project.',
        diagnostic: _diagnostic(result.stderr),
      );
    } on ProcessException catch (error) {
      return ProjectCreationFailure(
        'Could not start the Besfa CLI.',
        diagnostic: error.message,
      );
    } on FormatException {
      return const ProjectCreationFailure(
        'The Besfa CLI returned an invalid response.',
      );
    }
  }

  Map<String, dynamic> _decodeOutput(Object? stdout) {
    final decoded = jsonDecode(stdout.toString());
    if (decoded case Map<String, dynamic> value) {
      return value;
    }
    throw const FormatException('Expected a JSON object.');
  }

  String? _diagnostic(Object? stderr) {
    final text = stderr.toString().trim();
    return text.isEmpty ? null : text;
  }
}

sealed class ProjectCreationResult {
  const ProjectCreationResult();
}

class ProjectCreationSuccess extends ProjectCreationResult {
  const ProjectCreationSuccess(this.projectPath);

  final String projectPath;
}

class ProjectCreationFailure extends ProjectCreationResult {
  const ProjectCreationFailure(this.message, {this.diagnostic});

  final String message;
  final String? diagnostic;
}
