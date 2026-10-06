import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/features/update_plugin/model/plugin_update.dart';

void main() {
  late Directory dir;

  setUp(() => dir = Directory.systemTemp.createTempSync('besfa_plugin'));
  tearDown(() => dir.deleteSync(recursive: true));

  test('reads the git revision the lock pins the plugin to', () {
    final lock = File('${dir.path}/Cargo.lock')
      ..writeAsStringSync(
        '[[package]]\r\n'
        'name = "bevy"\r\n'
        'version = "0.19.1"\r\n'
        'source = "registry+https://github.com/rust-lang/crates.io-index"\r\n'
        '\r\n'
        '[[package]]\r\n'
        'name = "besfa_editor_plugin"\r\n'
        'version = "0.1.0"\r\n'
        'source = "git+https://github.com/ski0090/besfa#28b39927dda69b76"\r\n'
        'dependencies = [\r\n',
      );

    expect(pluginRevision(lock.path), '28b39927dda69b76');
  });

  test('has no revision without a lock or for a path dependency', () {
    expect(pluginRevision('${dir.path}/Cargo.lock'), isNull);

    final lock = File('${dir.path}/Cargo.lock')
      ..writeAsStringSync(
        '[[package]]\nname = "besfa_editor_plugin"\nversion = "0.1.0"\n'
        'dependencies = [\n',
      );
    expect(pluginRevision(lock.path), isNull);
  });
}
