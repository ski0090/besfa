// Renders the editor for the README's screenshots, with the fonts the
// running editor uses on Windows, and writes them to docs/images. Not part
// of `flutter test`; run it from apps/editor_ui after changing the UI:
//
//   flutter test tool/readme_screenshots_test.dart

import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/app/app.dart';
import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/features/prebuild_bevy/model/bevy_prebuild.dart';
import 'package:editor_ui/features/update_plugin/ui/plugin_update_view.dart';
import 'package:editor_ui/pages/project_editor/ui/project_editor_page.dart';

import '../test/support/editor_harness.dart';

const _old = '28b39927dda69b765ee1e7071794050033b4a307';
const _latest = '51b56bc0a1d9cd2bd1f1c5d6a2a8d5c9e0f1a2b3';

/// The window's size, at a pixel ratio that keeps text sharp.
const _window = Size(1280, 800);
const _pixelRatio = 1.5;

String _lock(String revision) =>
    '[[package]]\nname = "besfa_editor_plugin"\nversion = "0.1.0"\n'
    'source = "git+https://github.com/ski0090/besfa#$revision"\n';

/// What the template game prints and reports once it runs.
const _gameOutput = [
  r'   Compiling my_game v0.1.0 (C:\Projects\my_game)',
  "    Finished `dev` profile [optimized + debuginfo] target(s) in 5.67s",
  r'     Running `C:\Users\you\AppData\Local\Besfa\target\debug\my_game.exe`',
  '2026-10-07T05:36:08.737706Z  INFO besfa_editor_plugin::viewport: '
      'Rendering into the editor viewport 1280x720.',
  '2026-10-07T05:36:08.776701Z  INFO besfa_editor_plugin::scene: '
      'Loaded scenes/main.scn.ron',
  '@besfa {"type":"systems","crate":"my_game","systems":['
      '{"schedule":"Update","name":"my_game::spin"}]}',
  '@besfa {"type":"entities","entities":['
      '{"id":4294967295,"name":"Cube","scene":true},'
      '{"id":4294967294,"name":"Light","scene":true},'
      '{"id":4294967293,"name":"Camera","scene":true}]}',
];

Future<void> _loadFont(String family, List<String> files) async {
  final loader = FontLoader(family);
  for (final file in files) {
    loader.addFont(
      Future.value(ByteData.sublistView(File(file).readAsBytesSync())),
    );
  }
  await loader.load();
}

void main() {
  late Directory project;

  setUpAll(() async {
    const fonts = r'C:\Windows\Fonts';
    const segoe = [
      '$fonts\\segoeui.ttf',
      '$fonts\\seguisb.ttf',
      '$fonts\\segoeuib.ttf',
    ];
    await _loadFont('Segoe UI', segoe);
    // Text whose style names no font falls back to the test font here and
    // to Segoe UI on Windows.
    await _loadFont('FlutterTest', segoe);
    await _loadFont('Consolas', ['$fonts\\consola.ttf']);
    // The SDK's icon font, next to the flutter_tester running this.
    final artifacts = File(Platform.resolvedExecutable).parent.parent.parent;
    await _loadFont('MaterialIcons', [
      '${artifacts.path}/material_fonts/materialicons-regular.otf',
    ]);
  });

  setUp(() {
    project = Directory(
      '${Directory.systemTemp.createTempSync('besfa_shots').path}/my_game',
    )..createSync();
  });
  tearDown(() => project.parent.deleteSync(recursive: true));

  testWidgets('updating the plugin from the toolbar', (tester) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.windows;
    mockEditorChannels(tester);
    // A viewport texture, so the editor shows its scene view.
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      const MethodChannel('besfa/viewport'),
      (call) async => call.method == 'dispose'
          ? null
          : {'textureId': 1, 'name': 'viewport', 'adapter': 'gpu'},
    );
    tester.view.physicalSize = _window * _pixelRatio;
    tester.view.devicePixelRatio = _pixelRatio;
    addTearDown(tester.view.reset);

    final lock = File('${project.path}/Cargo.lock')
      ..writeAsStringSync(_lock(_old));
    final prebuild = BevyPrebuild()
      ..value = const BevyPrebuildStatus(
        BevyPrebuildPhase.ready,
        'Bevy is ready',
      );
    final screen = GlobalKey();
    await tester.pumpWidget(
      RepaintBoundary(
        key: screen,
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: editorTheme(),
          home: ProjectEditorPage(
            project: Project(project.path),
            prebuild: prebuild,
            latestPluginRevision: () => _latest,
            updatePlugin: (directory, {required onOutput}) async {
              onOutput(
                '    Updating git repository '
                '`https://github.com/ski0090/besfa`',
              );
              onOutput(
                '    Updating besfa_editor_plugin v0.1.0 '
                '(https://github.com/ski0090/besfa#${_old.substring(0, 8)}) '
                '-> #${_latest.substring(0, 8)}',
              );
              lock.writeAsStringSync(_lock(_latest));
              return 0;
            },
            startGame: (_, {required environment, required onOutput}) async {
              _gameOutput.forEach(onOutput);
              return FakeGame();
            },
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    // The project's plugin is older than the prebuild's: the toolbar warns,
    // and hovering explains what clicking does.
    final icon = find.byType(PluginUpdateView);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(tester.getCenter(icon));
    // Once to show it, once more for its fade-in to finish.
    await tester.pump(const Duration(seconds: 1));
    await tester.pump(const Duration(seconds: 1));
    expect(find.textContaining('differs from the Bevy prebuild'), findsOne);
    await _save(
      tester,
      screen,
      'plugin-update-available',
      crop: const Rect.fromLTWH(0, 0, 1280, 130),
      highlight: tester.getRect(icon),
    );

    // Clicking updates it, rebuilds the game and logs what cargo did.
    await mouse.moveTo(Offset.zero);
    await tester.tap(icon);
    await tester.pumpAndSettle();
    await _save(
      tester,
      screen,
      'plugin-updated',
      highlight: tester.getRect(icon),
    );
    await mouse.removePointer();
    debugDefaultTargetPlatformOverride = null;
  });
}

/// Writes the screen, or its [crop], to docs/images/[name].png, with a ring
/// around [highlight]. Rectangles are in logical pixels.
Future<void> _save(
  WidgetTester tester,
  GlobalKey screen,
  String name, {
  Rect? crop,
  Rect? highlight,
}) async {
  await tester.runAsync(() async {
    final boundary =
        screen.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await boundary.toImage(pixelRatio: _pixelRatio);
    Rect scaled(Rect rect) => Rect.fromLTRB(
      rect.left * _pixelRatio,
      rect.top * _pixelRatio,
      rect.right * _pixelRatio,
      rect.bottom * _pixelRatio,
    );
    final area = scaled(crop ?? Offset.zero & _window);
    final recorder = ui.PictureRecorder();
    final canvas = Canvas(recorder)
      ..drawImageRect(image, area, Offset.zero & area.size, Paint());
    if (highlight != null) {
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          scaled(highlight.inflate(3)).shift(-area.topLeft),
          const Radius.circular(8),
        ),
        Paint()
          ..color = const Color(0xFFFF5F57)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 3,
      );
    }
    final cropped = await recorder.endRecording().toImage(
      area.width.round(),
      area.height.round(),
    );
    final png = await cropped.toByteData(format: ui.ImageByteFormat.png);
    File('../../docs/images/$name.png')
      ..createSync(recursive: true)
      ..writeAsBytesSync(png!.buffer.asUint8List());
  });
}
