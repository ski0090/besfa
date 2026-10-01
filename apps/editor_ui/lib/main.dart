import 'package:flutter/widgets.dart';

import 'package:editor_ui/app/app.dart';
import 'package:editor_ui/app/window.dart';
import 'package:editor_ui/entities/project/model/recent_projects.dart';
import 'package:editor_ui/features/prebuild_bevy/model/bevy_prebuild.dart';

void main() async {
  await initializeEditorWindow();
  runApp(
    BesfaEditorApp(
      recentProjects: RecentProjects.standard(),
      prebuild: BevyPrebuild()..start(),
    ),
  );
}
