import 'dart:io';

import 'package:editor_ui/entities/project/model/project.dart';

/// Returns the project at [directory], or null if it is not a Cargo project.
// ponytail: Cargo.toml check only, switch to the .besfa marker once DATA_FORMAT.md is settled.
Project? loadProject(String directory) {
  final manifest = File('$directory${Platform.pathSeparator}Cargo.toml');
  return manifest.existsSync() ? Project(directory) : null;
}
