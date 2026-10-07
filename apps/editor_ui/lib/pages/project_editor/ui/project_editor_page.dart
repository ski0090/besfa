import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:window_manager/window_manager.dart';

import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/entities/scene/model/scene.dart';
import 'package:editor_ui/features/prebuild_bevy/model/bevy_prebuild.dart';
import 'package:editor_ui/features/prebuild_bevy/ui/bevy_prebuild_status_view.dart';
import 'package:editor_ui/features/update_plugin/model/plugin_update.dart';
import 'package:editor_ui/features/update_plugin/ui/plugin_update_view.dart';
import 'package:editor_ui/shared/native/viewport_texture.dart';
import 'package:editor_ui/shared/process/cli_process.dart';
import 'package:editor_ui/widgets/entity_inspector/ui/entity_inspector_panel.dart';
import 'package:editor_ui/widgets/scene_hierarchy/ui/scene_hierarchy_panel.dart';
import 'package:editor_ui/widgets/scene_viewport/ui/scene_viewport.dart';

/// Starts the game process in [directory]; tests replace it.
typedef StartGame =
    Future<CliProcess> Function(
      String directory, {
      required Map<String, String> environment,
      required void Function(String line) onOutput,
    });

Future<CliProcess> startCargoRun(
  String directory, {
  required Map<String, String> environment,
  required void Function(String line) onOutput,
}) => CliProcess.start(
  directory,
  executable: 'cargo',
  // Dynamic linking makes rebuilds after a code change much faster. Only
  // the editor turns it on, so a plain `cargo build` stays standalone.
  arguments: const ['run', '--features', 'bevy/dynamic_linking'],
  environment: environment,
  onOutput: onOutput,
);

class ProjectEditorPage extends StatefulWidget {
  const ProjectEditorPage({
    super.key,
    required this.project,
    this.prebuild,
    this.startGame = startCargoRun,
    this.latestPluginRevision = prebuildPluginRevision,
    this.updatePlugin = cargoUpdatePlugin,
  });

  final Project project;

  /// Shown in the toolbar; the edit session and Play wait for it.
  final BevyPrebuild? prebuild;

  final StartGame startGame;

  /// The `besfa_editor_plugin` revision the project is compared with.
  final String? Function() latestPluginRevision;

  final UpdatePlugin updatePlugin;

  @override
  State<ProjectEditorPage> createState() => _ProjectEditorPageState();
}

const _captionColor = Color(0xFF15171B);

// ponytail: fixed log cap, switch to a ring buffer if trimming shows up in profiles.
const _maxLogLines = 2000;

/// The viewport texture's size until the panel has been laid out.
const _defaultViewport = Size(1280, 720);

/// The scene view's handle tools, by the name the game takes, with their
/// icons and keys.
const _tools = [
  ('translate', Icons.open_with, 'Move (W)', LogicalKeyboardKey.keyW),
  ('rotate', Icons.rotate_right, 'Rotate (E)', LogicalKeyboardKey.keyE),
  ('scale', Icons.open_in_full, 'Scale (R)', LogicalKeyboardKey.keyR),
];

/// The game runs in one of two states: paused in edit mode, where the
/// viewport shows the scene as Startup built it, or playing. Stop ends play by
/// relaunching the game in edit mode, which resets the scene and picks up code
/// changes. In both states the game reports its entities, which the
/// hierarchy and inspector panels show and change through commands on its
/// stdin, one JSON object per line.
class _ProjectEditorPageState extends State<ProjectEditorPage> {
  final _logs = <String>[];
  final _scene = Scene();
  CliProcess? _game;
  ViewportTexture? _viewport;

  /// The viewport panel's size in physical pixels, which the texture follows.
  Size? _wantedViewport;
  Timer? _resizeTimer;

  /// Textures replaced by a resize. A running game may still render into
  /// one until it reads the `viewport` command, so they go when the next
  /// game launches.
  // ponytail: kept until the next launch; dispose on a game acknowledgment
  // if long sessions with many resizes hold too much GPU memory.
  final _retiredViewports = <ViewportTexture>[];

  /// The scene view's handle tool.
  String _tool = 'translate';

  /// Waiting for a game process: launching one, or stopping play.
  bool _starting = false;

  /// The game was told to play; otherwise it sits paused in edit mode.
  bool _playing = false;

  /// `besfa_editor_plugin` revisions: the project's and the prebuild's.
  String? _pluginRevision;
  String? _latestPluginRevision;

  @override
  void initState() {
    super.initState();
    HardwareKeyboard.instance.addHandler(_onKey);
    _readPluginRevisions();
    if (_prebuilding) {
      widget.prebuild!.addListener(_onPrebuildChanged);
    } else {
      _launch();
    }
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_onKey);
    widget.prebuild?.removeListener(_onPrebuildChanged);
    _resizeTimer?.cancel();
    _game?.stop();
    _viewport?.dispose();
    _disposeRetiredViewports();
    _scene.dispose();
    super.dispose();
  }

  void _onPrebuildChanged() {
    if (_prebuilding) {
      setState(() {});
      return;
    }
    widget.prebuild!.removeListener(_onPrebuildChanged);
    // The prebuild moved its lock to the latest plugin.
    _readPluginRevisions();
    _launch();
  }

  void _readPluginRevisions() {
    setState(() {
      _pluginRevision = pluginRevision(
        [widget.project.path, 'Cargo.lock'].join(Platform.pathSeparator),
      );
      _latestPluginRevision = widget.latestPluginRevision();
    });
  }

  /// The prebuild holds the shared build directory, so a game started now
  /// would only wait for it.
  bool get _prebuilding =>
      widget.prebuild?.value.phase == BevyPrebuildPhase.running;

  /// Reports from the game go to the scene, everything else to the log.
  void _onOutput(String line) {
    // A stopped game's last lines arrive after the page is disposed.
    if (!mounted) {
      return;
    }
    if (!_scene.handle(line)) {
      _log(line);
    }
  }

  void _log(String line) {
    if (!mounted) {
      return;
    }
    setState(() {
      _logs.add(line);
      if (_logs.length > _maxLogLines) {
        _logs.removeRange(0, _logs.length - _maxLogLines);
      }
    });
  }

  /// Starts the game paused in edit mode, or already playing with [play].
  Future<void> _launch({bool play = false}) async {
    setState(() => _starting = true);
    _scene.reset();
    // The previous game has exited, so nothing renders into them any more.
    _disposeRetiredViewports();
    final CliProcess game;
    try {
      final viewport = await _ensureViewport();
      game = await widget.startGame(
        widget.project.path,
        onOutput: _onOutput,
        environment: {
          // Read by besfa_editor_plugin: start paused, play on `play`.
          'BESFA_EDIT_MODE': '1',
          if (viewport != null) ...{
            // The plugin takes the size from the texture itself.
            'BESFA_VIEWPORT': viewport.sharedName,
            // The shared texture is opened on D3D12, on the editor's GPU.
            'WGPU_BACKEND': 'dx12',
            'WGPU_ADAPTER_NAME': viewport.adapterName,
          },
        },
      );
    } on ProcessException catch (error) {
      _log('Could not start cargo: ${error.message}');
      return;
    } finally {
      if (mounted) {
        setState(() => _starting = false);
      }
    }
    if (!mounted) {
      await game.stop();
      return;
    }
    if (play) {
      game.send(jsonEncode({'command': 'play'}));
    }
    // The scene is rebuilt from the same file, so the selection carries over.
    if (_scene.selected case final id?) {
      game.send(jsonEncode({'command': 'select', 'id': id}));
    }
    if (_tool != 'translate') {
      game.send(jsonEncode({'command': 'tool', 'tool': _tool}));
    }
    setState(() {
      _game = game;
      _playing = play;
    });
    final code = await game.exitCode;
    _log('Process exited with code $code.');
    // Stop may already have replaced it with a new edit session.
    if (mounted && _game == game) {
      setState(() {
        _game = null;
        _playing = false;
      });
    }
  }

  /// Sent while cargo still builds, `play` waits in the pipe for the game.
  /// Unsaved changes are saved first: Stop restarts from the file.
  void _play() {
    setState(() => _logs.clear());
    final game = _game;
    if (game == null) {
      // The edit session is gone, e.g. it failed to build or crashed.
      _launch(play: true);
      return;
    }
    if (_scene.dirty) {
      _save();
    }
    _send({'command': 'play'});
    setState(() => _playing = true);
  }

  void _send(Map<String, Object?> command) => _game?.send(jsonEncode(command));

  /// Sends a command that changes the scene. Changes made while playing
  /// end with the play session, so they do not count as unsaved.
  void _edit(Map<String, Object?> command) {
    _send(command);
    if (!_playing) {
      _scene.markDirty();
    }
  }

  /// The game reports whether saving worked, which clears the dirty mark.
  void _save() {
    if (_game != null && !_playing) {
      _send({'command': 'save'});
    }
  }

  /// The game reports the selected entity's components back.
  void _select(int? id) {
    _scene.select(id);
    _send({'command': 'select', 'id': id});
  }

  void _duplicate() {
    if (_scene.selectedEntity case final entity?) {
      _edit({'command': 'duplicate', 'id': entity.id});
    }
  }

  void _delete() {
    if (_scene.selectedEntity case final entity?) {
      _edit({'command': 'despawn', 'id': entity.id});
    }
  }

  void _setTool(String tool) {
    setState(() => _tool = tool);
    _send({'command': 'tool', 'tool': tool});
  }

  /// Scene shortcuts. Delete and Ctrl+D leave text fields alone, and a
  /// focused field is committed before Ctrl+S saves.
  bool _onKey(KeyEvent event) {
    if (event is! KeyDownEvent ||
        !mounted ||
        !(ModalRoute.of(context)?.isCurrent ?? false)) {
      return false;
    }
    final control = HardwareKeyboard.instance.isControlPressed;
    final key = event.logicalKey;
    if (control && key == LogicalKeyboardKey.keyS) {
      // Unfocusing commits the field in a microtask; save after it.
      FocusManager.instance.primaryFocus?.unfocus();
      Future.microtask(_save);
      return true;
    }
    final typing =
        FocusManager.instance.primaryFocus?.context
            ?.findAncestorWidgetOfExactType<EditableText>() !=
        null;
    if (typing) {
      return false;
    }
    if (control && key == LogicalKeyboardKey.keyD) {
      _duplicate();
      return true;
    }
    if (key == LogicalKeyboardKey.delete) {
      _delete();
      return true;
    }
    if (_playing || control) {
      return false;
    }
    for (final (tool, _, _, toolKey) in _tools) {
      if (key == toolKey) {
        _setTool(tool);
        return true;
      }
    }
    if (key == LogicalKeyboardKey.keyF) {
      _send({'command': 'focus'});
      return true;
    }
    return false;
  }

  Future<void> _leave() async {
    if (_scene.dirty) {
      final discard = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('Discard unsaved changes?'),
          content: const Text(
            'The scene has changes that are not saved to '
            'scenes/main.scn.ron.',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.of(context).pop(true),
              child: const Text('Discard'),
            ),
          ],
        ),
      );
      if (discard != true) {
        return;
      }
    }
    if (mounted) {
      Navigator.of(context).pop();
    }
  }

  /// Moves the project's plugin to the latest commit and restarts the edit
  /// session, which rebuilds the game against it.
  Future<void> _updatePlugin() async {
    setState(() => _starting = true);
    try {
      final code = await widget.updatePlugin(
        widget.project.path,
        onOutput: _log,
      );
      if (code != 0) {
        _log('cargo update exited with code $code.');
      }
    } on ProcessException catch (error) {
      _log('Could not start cargo: ${error.message}');
    }
    if (!mounted) {
      return;
    }
    _readPluginRevisions();
    if (_game case final game?) {
      await game.stop();
      // The running game locks its executable against the rebuild.
      await game.exitCode;
    }
    if (mounted) {
      _launch();
    }
  }

  Future<void> _stop() async {
    final game = _game!;
    setState(() => _starting = true);
    await game.stop();
    // The running game locks its executable against the next build.
    await game.exitCode;
    if (mounted) {
      _launch();
    }
  }

  /// Creates the viewport texture once per editor. Without it the game opens
  /// its own window, so failures are logged rather than fatal.
  Future<ViewportTexture?> _ensureViewport() async {
    if (_viewport != null) {
      return _viewport;
    }
    final size = _wantedViewport ?? _defaultViewport;
    try {
      final viewport = await ViewportTexture.create(
        width: size.width.round(),
        height: size.height.round(),
      );
      if (!mounted) {
        await viewport.dispose();
        return null;
      }
      setState(() => _viewport = viewport);
      return viewport;
    } on PlatformException catch (error) {
      _log(
        'Viewport unavailable, the game opens its own window: '
        '${error.message}',
      );
      return null;
    }
  }

  /// Follows the viewport panel's size, [physical] pixels, once it settles.
  void _fitViewport(Size physical) {
    final size = Size(
      physical.width.clamp(16, 4096).roundToDouble(),
      physical.height.clamp(16, 4096).roundToDouble(),
    );
    if (size == _wantedViewport) {
      return;
    }
    _wantedViewport = size;
    _resizeTimer?.cancel();
    _resizeTimer = Timer(
      const Duration(milliseconds: 250),
      () => _resizeViewport(size),
    );
  }

  /// Makes a texture of [size], shows it, and tells the game to render into
  /// it. The old texture is kept until the next launch.
  Future<void> _resizeViewport(Size size) async {
    final old = _viewport;
    if (old == null ||
        (old.width == size.width.round() &&
            old.height == size.height.round())) {
      return;
    }
    final ViewportTexture viewport;
    try {
      viewport = await ViewportTexture.create(
        width: size.width.round(),
        height: size.height.round(),
      );
    } on PlatformException catch (error) {
      _log('Could not resize the viewport: ${error.message}');
      return;
    }
    // Disposed meanwhile, or resized again while this one was made.
    if (!mounted || size != _wantedViewport || _viewport != old) {
      await viewport.dispose();
      return;
    }
    _retiredViewports.add(old);
    setState(() => _viewport = viewport);
    _send({'command': 'viewport', 'name': viewport.sharedName});
  }

  void _disposeRetiredViewports() {
    for (final viewport in _retiredViewports) {
      viewport.dispose();
    }
    _retiredViewports.clear();
  }

  @override
  Widget build(BuildContext context) {
    final viewport = _viewport;
    return Scaffold(
      appBar: PreferredSize(
        preferredSize: const Size.fromHeight(kWindowCaptionHeight),
        child: ColoredBox(
          color: _captionColor,
          child: Stack(
            children: [
              Row(
                children: [
                  // Kept outside WindowCaption: its drag area waits for a
                  // double tap, which would delay every button press inside it.
                  MenuBar(
                    style: const MenuStyle(
                      backgroundColor: WidgetStatePropertyAll(_captionColor),
                      elevation: WidgetStatePropertyAll(0),
                    ),
                    children: [
                      SubmenuButton(
                        menuChildren: [
                          MenuItemButton(
                            leadingIcon: const Icon(Icons.save, size: 16),
                            trailingIcon: const Text('Ctrl+S'),
                            // Only the edit session's scene is worth keeping.
                            onPressed: _game != null && !_playing
                                ? _save
                                : null,
                            child: const Text('Save scene'),
                          ),
                          MenuItemButton(
                            leadingIcon: const Icon(Icons.arrow_back, size: 16),
                            onPressed: _leave,
                            child: const Text('Back to Project Hub'),
                          ),
                        ],
                        child: const Text('File'),
                      ),
                    ],
                  ),
                  Expanded(
                    child: WindowCaption(
                      brightness: Theme.of(context).brightness,
                      backgroundColor: _captionColor,
                    ),
                  ),
                ],
              ),
              // Centered on the whole bar; ignores pointers so dragging it
              // still moves the window.
              IgnorePointer(
                child: Center(
                  child: ListenableBuilder(
                    listenable: _scene,
                    builder: (context, _) =>
                        _ProjectTitle(widget.project.name, dirty: _scene.dirty),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
      body: Column(
        children: [
          _RunToolbar(
            playing: _playing,
            busy: _starting || _prebuilding,
            onPlay: _play,
            onStop: _stop,
            tool: _tool,
            onTool: _setTool,
            plugin: PluginUpdateView(
              project: _pluginRevision,
              latest: _latestPluginRevision,
              busy: _starting || _prebuilding,
              onUpdate: _updatePlugin,
            ),
            prebuild: widget.prebuild,
          ),
          Expanded(
            child: Row(
              children: [
                SizedBox(
                  width: 220,
                  child: SceneHierarchyPanel(
                    scene: _scene,
                    onSelect: _select,
                    onSpawn: (kind) =>
                        _edit({'command': 'spawn', 'kind': kind}),
                    onDuplicate: _duplicate,
                    onDelete: _delete,
                  ),
                ),
                const _PanelDivider(),
                Expanded(
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      _fitViewport(
                        constraints.biggest *
                            MediaQuery.devicePixelRatioOf(context),
                      );
                      return Center(
                        child: _game != null && viewport != null
                            ? AspectRatio(
                                aspectRatio: viewport.width / viewport.height,
                                child: Stack(
                                  fit: StackFit.expand,
                                  children: [
                                    SceneViewport(
                                      textureId: viewport.textureId,
                                      textureSize: Size(
                                        viewport.width.toDouble(),
                                        viewport.height.toDouble(),
                                      ),
                                      onCommand: _send,
                                    ),
                                    if (!_playing) const _ViewportHint(),
                                  ],
                                ),
                              )
                            : const Text(
                                'Viewport',
                                style: TextStyle(color: Color(0xFF7E8795)),
                              ),
                      );
                    },
                  ),
                ),
                const _PanelDivider(),
                SizedBox(
                  width: 320,
                  child: EntityInspectorPanel(
                    scene: _scene,
                    onSet: (component, value) => _edit({
                      'command': 'set',
                      'id': _scene.selected,
                      'component': component,
                      'value': value,
                    }),
                    onInsert: (component) => _edit({
                      'command': 'insert',
                      'id': _scene.selected,
                      'component': component,
                    }),
                    onRemove: (component) => _edit({
                      'command': 'remove',
                      'id': _scene.selected,
                      'component': component,
                    }),
                  ),
                ),
              ],
            ),
          ),
          SizedBox(height: 200, child: _LogPanel(_logs)),
        ],
      ),
    );
  }
}

class _RunToolbar extends StatelessWidget {
  const _RunToolbar({
    required this.playing,
    required this.busy,
    required this.onPlay,
    required this.onStop,
    required this.tool,
    required this.onTool,
    required this.plugin,
    this.prebuild,
  });

  final bool playing;
  final bool busy;
  final VoidCallback onPlay;
  final VoidCallback onStop;
  final String tool;
  final ValueChanged<String> onTool;
  final Widget plugin;
  final BevyPrebuild? prebuild;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 40,
      padding: const EdgeInsets.symmetric(horizontal: 8),
      decoration: BoxDecoration(
        border: Border(
          bottom: BorderSide(color: Colors.white.withValues(alpha: .08)),
        ),
      ),
      child: Row(
        children: [
          playing
              ? TextButton.icon(
                  onPressed: busy ? null : onStop,
                  icon: const Icon(Icons.stop, size: 18),
                  label: const Text('Stop'),
                )
              : TextButton.icon(
                  onPressed: busy ? null : onPlay,
                  icon: const Icon(Icons.play_arrow, size: 18),
                  label: const Text('Play'),
                ),
          const SizedBox(width: 12),
          for (final (name, icon, tooltip, _) in _tools)
            IconButton(
              icon: Icon(icon, size: 18),
              tooltip: tooltip,
              isSelected: tool == name,
              visualDensity: VisualDensity.compact,
              style:
                  IconButton.styleFrom(
                    foregroundColor: const Color(0xFF7E8795),
                  ).copyWith(
                    backgroundColor: WidgetStateProperty.resolveWith(
                      (states) => states.contains(WidgetState.selected)
                          ? const Color(0x338CB4FF)
                          : null,
                    ),
                  ),
              onPressed: playing ? null : () => onTool(name),
            ),
          Expanded(
            child: Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                plugin,
                // Bounded, so a long cargo line ellipsizes instead of overflowing.
                if (prebuild case final prebuild?)
                  Flexible(
                    child: Padding(
                      padding: const EdgeInsets.only(left: 16),
                      child: BevyPrebuildStatusView(prebuild),
                    ),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// How to move around the scene view, over its corner.
class _ViewportHint extends StatelessWidget {
  const _ViewportHint();

  @override
  Widget build(BuildContext context) {
    return const IgnorePointer(
      child: Align(
        alignment: Alignment.bottomLeft,
        child: Padding(
          padding: EdgeInsets.all(8),
          child: Text(
            'Click select  ·  Right-drag orbit  ·  Middle-drag pan  ·  '
            'Scroll zoom  ·  F focus',
            style: TextStyle(fontSize: 11, color: Color(0x99FFFFFF)),
          ),
        ),
      ),
    );
  }
}

class _PanelDivider extends StatelessWidget {
  const _PanelDivider();

  @override
  Widget build(BuildContext context) =>
      Container(width: 1, color: Colors.white.withValues(alpha: .08));
}

class _LogPanel extends StatelessWidget {
  const _LogPanel(this.lines);

  final List<String> lines;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: const Color(0xFF111316),
        border: Border(
          top: BorderSide(color: Colors.white.withValues(alpha: .08)),
        ),
      ),
      // Reversed so the newest line stays pinned to the bottom.
      child: ListView.builder(
        reverse: true,
        padding: const EdgeInsets.all(8),
        itemCount: lines.length,
        itemBuilder: (context, index) => Text(
          lines[lines.length - 1 - index],
          style: const TextStyle(
            fontFamily: 'Consolas',
            fontSize: 12,
            color: Color(0xFFB3BBC8),
          ),
        ),
      ),
    );
  }
}

class _ProjectTitle extends StatelessWidget {
  const _ProjectTitle(this.name, {required this.dirty});

  final String name;

  /// The scene has changes that are not saved.
  final bool dirty;

  @override
  Widget build(BuildContext context) {
    return Container(
      constraints: const BoxConstraints(minWidth: 240, maxWidth: 480),
      height: 22,
      padding: const EdgeInsets.symmetric(horizontal: 12),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: Colors.white.withValues(alpha: .04),
        border: Border.all(color: Colors.white.withValues(alpha: .14)),
        borderRadius: BorderRadius.circular(6),
      ),
      child: Text(
        dirty ? '$name *' : name,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(fontSize: 12, color: Color(0xFFB3BBC8)),
      ),
    );
  }
}
