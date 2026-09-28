import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/entities/project/model/recent_projects.dart';

void main() {
  late Directory dir;
  late RecentProjects recent;

  setUp(() {
    dir = Directory.systemTemp.createTempSync('besfa_recent');
    recent = RecentProjects(File('${dir.path}/nested/recent.json'));
  });
  tearDown(() => dir.deleteSync(recursive: true));

  List<String> paths() => [for (final p in recent.load()) p.path];

  test('starts empty and survives a corrupt file', () {
    expect(recent.load(), isEmpty);
    recent.file
      ..parent.createSync(recursive: true)
      ..writeAsStringSync('{not json');
    expect(recent.load(), isEmpty);
  });

  test('keeps newest first, dedupes ignoring case, and caps the list', () {
    recent.add(const Project(r'C:\a'));
    recent.add(const Project(r'C:\b'));
    recent.add(const Project(r'c:\A'));
    expect(paths(), [r'c:\A', r'C:\b']);

    for (var i = 0; i < RecentProjects.limit + 5; i++) {
      recent.add(Project('C:\\p$i'));
    }
    expect(paths(), hasLength(RecentProjects.limit));
    expect(paths().first, 'C:\\p${RecentProjects.limit + 4}');

    recent.remove(Project(paths().first));
    expect(paths(), hasLength(RecentProjects.limit - 1));
  });
}
