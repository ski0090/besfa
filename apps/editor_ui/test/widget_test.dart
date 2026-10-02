import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/app/app.dart';
import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/entities/project/model/recent_projects.dart';
import 'package:editor_ui/features/create_project/model/project_creator.dart';
import 'package:editor_ui/features/prebuild_bevy/model/bevy_prebuild.dart';
import 'package:editor_ui/pages/project_editor/ui/project_editor_page.dart';
import 'package:editor_ui/shared/process/cli_process.dart';

void main() {
  late Directory dir;
  late RecentProjects recent;

  setUp(() {
    dir = Directory.systemTemp.createTempSync('besfa_widget');
    recent = RecentProjects(File('${dir.path}/recent.json'));
  });
  tearDown(() => dir.deleteSync(recursive: true));

  testWidgets('shows the project hub', (WidgetTester tester) async {
    await tester.pumpWidget(BesfaEditorApp(recentProjects: recent));

    expect(find.text('Start creating'), findsOneWidget);
    expect(find.text('Create project'), findsOneWidget);
    expect(find.text('Open project'), findsOneWidget);
  });

  testWidgets('creates a project and opens it in the editor', (
    WidgetTester tester,
  ) async {
    _mockEditorChannels(tester);

    await tester.pumpWidget(
      BesfaEditorApp(
        projectCreator: _FakeProjectCreator(),
        recentProjects: recent,
        startGame: (_, {required environment, required onOutput}) =>
            Completer<CliProcess>().future,
      ),
    );

    await tester.tap(find.widgetWithText(FilledButton, 'New project'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), r'C:\Projects\demo_game');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pumpAndSettle();

    expect(find.text('Viewport'), findsOneWidget);
    expect(find.text('demo_game'), findsOneWidget);

    await tester.tap(find.text('File'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Back to Project Hub'));
    await tester.pumpAndSettle();
    expect(find.text('Start creating'), findsOneWidget);
    expect(find.text(r'C:\Projects\demo_game'), findsOneWidget);
    expect(recent.load().single.path, r'C:\Projects\demo_game');
  });

  testWidgets('the edit session waits for the Bevy prebuild', (
    WidgetTester tester,
  ) async {
    _mockEditorChannels(tester);
    final prebuild = BevyPrebuild();
    final launches = <Map<String, String>>[];
    await tester.pumpWidget(
      MaterialApp(
        home: ProjectEditorPage(
          project: Project(dir.path),
          prebuild: prebuild,
          startGame: (_, {required environment, required onOutput}) {
            launches.add(environment);
            return Completer<CliProcess>().future;
          },
        ),
      ),
    );
    final play = find.widgetWithText(TextButton, 'Play');

    expect(tester.widget<TextButton>(play).enabled, isFalse);
    expect(find.text('Preparing Bevy…'), findsOneWidget);
    expect(launches, isEmpty);

    prebuild.value = const BevyPrebuildStatus(
      BevyPrebuildPhase.ready,
      'Bevy is ready',
    );
    await tester.pump();
    expect(launches.single['BESFA_EDIT_MODE'], '1');
    expect(find.text('Bevy is ready'), findsOneWidget);
  });

  testWidgets('deletes a recent project only after confirmation', (
    WidgetTester tester,
  ) async {
    final project = Directory('${dir.path}/old_game')..createSync();
    File('${project.path}/Cargo.toml').createSync();
    recent.add(Project(project.path));
    final deleted = <String>[];
    await tester.pumpWidget(
      BesfaEditorApp(
        recentProjects: recent,
        deleteProject: (path) async {
          deleted.add(path);
          return true;
        },
      ),
    );

    final delete = find.byTooltip('Delete project');
    await tester.ensureVisible(delete);
    await tester.tap(delete);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(deleted, isEmpty);
    expect(recent.load(), hasLength(1));

    await tester.tap(delete);
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Delete'));
    await tester.pumpAndSettle();
    expect(deleted, [project.path]);
    expect(find.text('No recent projects'), findsOneWidget);
    expect(recent.load(), isEmpty);
  });
}

void _mockEditorChannels(WidgetTester tester) {
  final messenger = tester.binding.defaultBinaryMessenger;
  messenger.setMockMethodCallHandler(
    const MethodChannel('window_manager'),
    (call) async => call.method == 'isMaximized' ? false : null,
  );
  messenger.setMockMethodCallHandler(
    const MethodChannel('besfa/viewport'),
    (call) async => throw PlatformException(code: 'unavailable'),
  );
}

class _FakeProjectCreator implements ProjectCreator {
  @override
  Future<ProjectCreationResult> create(String directory) async {
    return ProjectCreationSuccess(directory);
  }
}
