import 'dart:convert';
import 'dart:io';

import 'package:editor_ui/entities/project/model/project.dart';

/// Recently opened projects, newest first, stored as a JSON list of paths.
class RecentProjects {
  RecentProjects(this.file);

  RecentProjects.standard()
    : this(
        File(
          [
            Platform.environment['APPDATA'] ??
                Platform.environment['HOME'] ??
                '.',
            'Besfa',
            'recent_projects.json',
          ].join(Platform.pathSeparator),
        ),
      );

  static const limit = 10;

  final File file;

  List<Project> load() {
    try {
      final paths = jsonDecode(file.readAsStringSync()) as List<dynamic>;
      return [for (final path in paths.whereType<String>()) Project(path)];
    } on FileSystemException {
      return [];
    } on FormatException {
      return [];
    } on TypeError {
      return [];
    }
  }

  /// Moves [project] to the top of the list and returns the new list.
  List<Project> add(Project project) =>
      _save([project, ..._without(project)].take(limit).toList());

  List<Project> remove(Project project) => _save(_without(project).toList());

  // Windows paths are case-insensitive.
  Iterable<Project> _without(Project project) => load().where(
    (other) => other.path.toLowerCase() != project.path.toLowerCase(),
  );

  List<Project> _save(List<Project> projects) {
    try {
      file.parent.createSync(recursive: true);
      file.writeAsStringSync(
        jsonEncode([for (final project in projects) project.path]),
      );
    } on FileSystemException {
      // The list is a convenience; failing to save must not block opening.
    }
    return projects;
  }
}
