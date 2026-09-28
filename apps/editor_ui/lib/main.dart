import 'package:flutter/widgets.dart';

import 'package:editor_ui/app/app.dart';
import 'package:editor_ui/app/window.dart';
import 'package:editor_ui/entities/project/model/recent_projects.dart';

void main() async {
  await initializeEditorWindow();
  runApp(BesfaEditorApp(recentProjects: RecentProjects.standard()));
}
