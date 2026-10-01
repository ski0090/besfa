import 'package:flutter/material.dart';

import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/entities/project/model/recent_projects.dart';
import 'package:editor_ui/features/create_project/model/project_creator.dart';
import 'package:editor_ui/features/delete_project/model/project_deleter.dart';
import 'package:editor_ui/features/prebuild_bevy/model/bevy_prebuild.dart';
import 'package:editor_ui/pages/project_editor/ui/project_editor_page.dart';
import 'package:editor_ui/pages/project_hub/ui/project_hub_page.dart';

class BesfaEditorApp extends StatelessWidget {
  const BesfaEditorApp({
    super.key,
    this.projectCreator = const BesfaCliProjectCreator(),
    required this.recentProjects,
    this.deleteProject = moveToRecycleBin,
    this.prebuild,
  });

  final ProjectCreator projectCreator;
  final RecentProjects recentProjects;
  final Future<bool> Function(String directory) deleteProject;

  /// Progress of the background Bevy prebuild, shown on every page.
  final BevyPrebuild? prebuild;

  @override
  Widget build(BuildContext context) {
    const colorScheme = ColorScheme.dark(
      primary: Color(0xFF8CB4FF),
      onPrimary: Color(0xFF10264C),
      surface: Color(0xFF20242B),
      onSurface: Color(0xFFE4E7EC),
    );

    return MaterialApp(
      title: 'Besfa',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        useMaterial3: true,
        brightness: Brightness.dark,
        colorScheme: colorScheme,
        scaffoldBackgroundColor: const Color(0xFF15171B),
        cardTheme: const CardThemeData(
          margin: EdgeInsets.zero,
          elevation: 0,
          color: Color(0xFF20242B),
        ),
      ),
      home: ProjectHubPage(
        projectCreator: projectCreator,
        recentProjects: recentProjects,
        deleteProject: deleteProject,
        prebuild: prebuild,
      ),
      onGenerateRoute: (settings) => switch (settings) {
        RouteSettings(
          name: ProjectHubPage.editorRoute,
          :final Project arguments,
        ) =>
          MaterialPageRoute<void>(
            settings: settings,
            builder: (_) =>
                ProjectEditorPage(project: arguments, prebuild: prebuild),
          ),
        _ => null,
      },
    );
  }
}
