import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';

import 'package:editor_ui/shared/ui/panel.dart';

/// Files the scene can show: glTF models, and scene and prefab files.
bool canPlace(String path) =>
    path.endsWith('.glb') ||
    path.endsWith('.gltf') ||
    path.endsWith('.scn.ron');

/// One row of the tree: a path relative to the asset folder, with forward
/// slashes as asset paths have them.
typedef _Row = ({String path, String name, int depth, bool folder});

/// The project's asset folder as a tree that follows changes on disk.
/// Models, scenes and prefabs can be added to the scene with a button or
/// dragged out as their asset path.
class AssetBrowser extends StatefulWidget {
  const AssetBrowser({super.key, required this.directory, this.onAdd});

  final Directory directory;

  /// An asset path [canPlace] accepts.
  final ValueChanged<String>? onAdd;

  @override
  State<AssetBrowser> createState() => _AssetBrowserState();
}

class _AssetBrowserState extends State<AssetBrowser> {
  /// Null while there is no asset folder.
  List<_Row>? _rows;
  StreamSubscription<FileSystemEvent>? _watch;
  Timer? _reload;

  @override
  void initState() {
    super.initState();
    _load(initial: true);
  }

  @override
  void didUpdateWidget(AssetBrowser oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.directory.path != widget.directory.path) {
      _load();
    }
  }

  @override
  void dispose() {
    _watch?.cancel();
    _reload?.cancel();
    super.dispose();
  }

  /// Lists the folder and watches it for changes.
  void _load({bool initial = false}) {
    final directory = widget.directory;
    final rows = directory.existsSync() ? _list(directory, '', 0) : null;
    if (initial) {
      _rows = rows;
    } else {
      setState(() => _rows = rows);
    }
    _watch?.cancel();
    _watch = null;
    if (rows != null) {
      // A burst of changes, like a prefab being written, reloads once.
      _watch = directory.watch(recursive: true).listen((_) {
        _reload?.cancel();
        _reload = Timer(const Duration(milliseconds: 200), () {
          if (mounted) {
            _load();
          }
        });
      }, onError: (_) {});
    }
  }

  @override
  Widget build(BuildContext context) {
    final rows = _rows;
    if (rows == null) {
      return Center(
        child: TextButton.icon(
          icon: const Icon(Icons.refresh, size: 16),
          label: const Text('No assets folder'),
          onPressed: _load,
        ),
      );
    }
    return ListView.builder(
      padding: const EdgeInsets.symmetric(vertical: 4),
      itemCount: rows.length,
      itemBuilder: (context, index) {
        final row = rows[index];
        final placeable = !row.folder && canPlace(row.path);
        final onAdd = widget.onAdd;
        final label = Row(
          children: [
            Icon(
              row.folder
                  ? Icons.folder_outlined
                  : placeable
                  ? Icons.view_in_ar_outlined
                  : Icons.insert_drive_file_outlined,
              size: 16,
              color: panelMutedText,
            ),
            const SizedBox(width: 6),
            Flexible(
              child: Text(
                row.name,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontSize: 12, color: panelText),
              ),
            ),
          ],
        );
        return Container(
          height: 26,
          padding: EdgeInsets.only(left: 8 + row.depth * 16.0, right: 4),
          child: Row(
            children: [
              Expanded(
                child: placeable && onAdd != null
                    ? Draggable<String>(
                        data: row.path,
                        feedback: Material(
                          color: const Color(0xFF20242B),
                          borderRadius: BorderRadius.circular(4),
                          child: Padding(
                            padding: const EdgeInsets.all(6),
                            child: Text(
                              row.name,
                              style: const TextStyle(
                                fontSize: 12,
                                color: panelText,
                              ),
                            ),
                          ),
                        ),
                        child: label,
                      )
                    : label,
              ),
              if (placeable && onAdd != null)
                PanelAction(
                  icon: Icons.add,
                  tooltip: 'Add to scene',
                  onPressed: () => onAdd(row.path),
                ),
            ],
          ),
        );
      },
    );
  }
}

/// [directory]'s folders, then its files, each folder followed by what is
/// in it. Hidden files are left out.
List<_Row> _list(Directory directory, String prefix, int depth) {
  String nameOf(FileSystemEntity entity) =>
      entity.path.split(RegExp(r'[\\/]')).lastWhere((part) => part.isNotEmpty);
  final entries = [
    for (final entity in directory.listSync())
      if (!nameOf(entity).startsWith('.')) entity,
  ]..sort((a, b) => nameOf(a).toLowerCase().compareTo(nameOf(b).toLowerCase()));
  return [
    for (final folder in entries.whereType<Directory>()) ...[
      (
        path: '$prefix${nameOf(folder)}',
        name: nameOf(folder),
        depth: depth,
        folder: true,
      ),
      ..._list(folder, '$prefix${nameOf(folder)}/', depth + 1),
    ],
    for (final file in entries.whereType<File>())
      (
        path: '$prefix${nameOf(file)}',
        name: nameOf(file),
        depth: depth,
        folder: false,
      ),
  ];
}
