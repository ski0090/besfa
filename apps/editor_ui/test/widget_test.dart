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
import 'package:editor_ui/widgets/scene_hierarchy/ui/scene_hierarchy_panel.dart';

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

    // A long cargo line must not overflow the toolbar.
    prebuild.value = BevyPrebuildStatus(
      BevyPrebuildPhase.running,
      'Compiling bevy_render v0.19.1 ' * 12,
      progress: .5,
    );
    await tester.pump();

    prebuild.value = const BevyPrebuildStatus(
      BevyPrebuildPhase.ready,
      'Bevy is ready',
    );
    await tester.pump();
    expect(launches.single['BESFA_EDIT_MODE'], '1');
    expect(find.text('Bevy is ready'), findsOneWidget);
  });

  testWidgets('selecting an entity shows what the game reports about it', (
    WidgetTester tester,
  ) async {
    _mockEditorChannels(tester);
    final game = _FakeGame();
    late void Function(String line) output;
    await tester.pumpWidget(
      MaterialApp(
        home: ProjectEditorPage(
          project: Project(dir.path),
          startGame: (_, {required environment, required onOutput}) async {
            output = onOutput;
            return game;
          },
        ),
      ),
    );
    await tester.pump();
    output(
      '@besfa {"type":"systems","crate":"demo_game","systems":['
      '{"schedule":"Update","name":"demo_game::spin"}]}',
    );
    output(
      '@besfa {"type":"entities","entities":['
      '{"id":4294967295,"name":"Cube","parent":null},'
      '{"id":4294967294,"name":null,"parent":4294967295}]}',
    );
    output('INFO demo_game: hello');
    await tester.pump();

    expect(find.text('Select an entity in the Hierarchy'), findsOneWidget);
    expect(find.text('demo_game (1)'), findsOneWidget);
    expect(find.text('Entity 1v0'), findsOneWidget);
    expect(find.text('INFO demo_game: hello'), findsOneWidget);
    expect(find.textContaining('@besfa'), findsNothing);

    // The inspector header repeats the name once selected.
    final cubeRow = find.descendant(
      of: find.byType(SceneHierarchyPanel),
      matching: find.text('Cube'),
    );
    await tester.tap(cubeRow);
    await tester.pump();
    expect(game.sent, ['select 4294967295']);

    output(
      '@besfa {"type":"entity","id":4294967295,"components":['
      '{"name":"Transform","path":"bevy_transform::components::transform::Transform",'
      '"mutable":true,"required_by":null,"value":{"translation":[0.0,1.0,0.0]}},'
      '{"name":"Spin","path":"demo_game::Spin","mutable":true,"required_by":null,"value":null},'
      '{"name":"GlobalTransform","path":"bevy_transform::components::global_transform::GlobalTransform",'
      '"mutable":true,"required_by":"Transform","value":"opaque"}]}',
    );
    await tester.pumpAndSettle();

    expect(find.text('Spin'), findsOneWidget);
    expect(find.text('no Reflect'), findsOneWidget);
    expect(find.text('bevy_transform (2)'), findsOneWidget);
    // Bevy's crates start collapsed.
    expect(find.text('Transform'), findsNothing);
    await tester.tap(find.text('bevy_transform (2)'));
    await tester.pumpAndSettle();
    expect(find.text('Transform'), findsOneWidget);
    expect(find.text('required by Transform'), findsOneWidget);
    expect(find.textContaining('"translation"'), findsOneWidget);

    await tester.tap(cubeRow);
    await tester.pump();
    expect(game.sent.last, 'select');
    expect(find.text('Select an entity in the Hierarchy'), findsOneWidget);

    // The stopped game's last lines arrive after the page is gone.
    await tester.pumpWidget(const SizedBox());
    output('@besfa {"type":"entities","entities":[]}');
    output('INFO demo_game: bye');
  });

  testWidgets('updates the plugin from the toolbar and relaunches the game', (
    WidgetTester tester,
  ) async {
    _mockEditorChannels(tester);
    const old = 'aaaaaaa1111111111111111111111111111111111';
    const latest = 'bbbbbbb2222222222222222222222222222222222';
    final lock = File('${dir.path}/Cargo.lock');
    String pin(String revision) =>
        '[[package]]\nname = "besfa_editor_plugin"\nversion = "0.1.0"\n'
        'source = "git+https://github.com/ski0090/besfa#$revision"\n';
    lock.writeAsStringSync(pin(old));
    var launches = 0;
    final updated = <String>[];
    await tester.pumpWidget(
      MaterialApp(
        home: ProjectEditorPage(
          project: Project(dir.path),
          latestPluginRevision: () => latest,
          updatePlugin: (directory, {required onOutput}) async {
            updated.add(directory);
            lock.writeAsStringSync(pin(latest));
            onOutput('    Updating besfa_editor_plugin');
            return 0;
          },
          startGame: (_, {required environment, required onOutput}) async {
            launches++;
            return _FakeGame();
          },
        ),
      ),
    );
    await tester.pump();

    expect(find.text('Plugin aaaaaaa, prebuild bbbbbbb'), findsOneWidget);
    await tester.tap(find.widgetWithText(TextButton, 'Update'));
    await tester.pumpAndSettle();

    expect(updated, [dir.path]);
    expect(find.text('Plugin bbbbbbb'), findsOneWidget);
    expect(find.text('    Updating besfa_editor_plugin'), findsOneWidget);
    expect(launches, 2);
    // Up to date: the refresh icon runs the same update.
    await tester.tap(find.byIcon(Icons.refresh));
    await tester.pumpAndSettle();
    expect(updated, hasLength(2));
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

/// A game that runs until stopped and records what the editor sends it.
class _FakeGame implements CliProcess {
  final sent = <String>[];
  final _exit = Completer<int>();

  @override
  Future<int> get exitCode => _exit.future;

  @override
  void send(String line) => sent.add(line);

  /// Exits the way a killed process does.
  @override
  Future<void> stop() async {
    if (!_exit.isCompleted) {
      _exit.complete(1);
    }
  }
}

class _FakeProjectCreator implements ProjectCreator {
  @override
  Future<ProjectCreationResult> create(String directory) async {
    return ProjectCreationSuccess(directory);
  }
}
