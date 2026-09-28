import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/features/open_project/model/project_loader.dart';

void main() {
  test('loads only folders with a Cargo.toml', () {
    final dir = Directory.systemTemp.createTempSync('besfa_game');
    addTearDown(() => dir.deleteSync(recursive: true));

    expect(loadProject(dir.path), isNull);

    File(
      '${dir.path}${Platform.pathSeparator}Cargo.toml',
    ).writeAsStringSync('');
    expect(
      loadProject(dir.path)?.name,
      dir.path.split(Platform.pathSeparator).last,
    );
  });
}
