import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:window_manager/window_manager.dart';

import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/features/prebuild_bevy/model/bevy_prebuild.dart';
import 'package:editor_ui/features/prebuild_bevy/ui/bevy_prebuild_status_view.dart';
import 'package:editor_ui/shared/native/viewport_texture.dart';
import 'package:editor_ui/shared/process/cli_process.dart';

class ProjectEditorPage extends StatefulWidget {
  const ProjectEditorPage({super.key, required this.project, this.prebuild});

  final Project project;

  /// Shown in the toolbar; Run is disabled while it is running.
  final BevyPrebuild? prebuild;

  @override
  State<ProjectEditorPage> createState() => _ProjectEditorPageState();
}

const _captionColor = Color(0xFF15171B);

// ponytail: fixed log cap, switch to a ring buffer if trimming shows up in profiles.
const _maxLogLines = 2000;

// ponytail: fixed viewport resolution, resize the shared texture with the
// panel once the viewport needs to fill it exactly.
const _viewportWidth = 1280;
const _viewportHeight = 720;

class _ProjectEditorPageState extends State<ProjectEditorPage> {
  final _logs = <String>[];
  CliProcess? _game;
  ViewportTexture? _viewport;
  bool _starting = false;

  @override
  void initState() {
    super.initState();
    widget.prebuild?.addListener(_onPrebuildChanged);
  }

  @override
  void dispose() {
    widget.prebuild?.removeListener(_onPrebuildChanged);
    _game?.stop();
    _viewport?.dispose();
    super.dispose();
  }

  void _onPrebuildChanged() => setState(() {});

  /// The prebuild holds the shared build directory, so a Run now would only
  /// wait for it.
  bool get _prebuilding =>
      widget.prebuild?.value.phase == BevyPrebuildPhase.running;

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

  Future<void> _run() async {
    setState(() {
      _logs.clear();
      _starting = true;
    });
    try {
      final viewport = await _ensureViewport();
      final game = await CliProcess.start(
        widget.project.path,
        executable: 'cargo',
        // Dynamic linking makes rebuilds after a code change much faster. Only
        // the editor turns it on, so a plain `cargo build` stays standalone.
        arguments: const ['run', '--features', 'bevy/dynamic_linking'],
        onOutput: _log,
        environment: viewport == null
            ? null
            : {
                // Read by besfa_editor_plugin in the game, which takes the
                // size from the texture itself.
                'BESFA_VIEWPORT': viewport.sharedName,
                // The shared texture is opened on D3D12, on the editor's GPU.
                'WGPU_BACKEND': 'dx12',
                'WGPU_ADAPTER_NAME': viewport.adapterName,
              },
      );
      if (!mounted) {
        await game.stop();
        return;
      }
      setState(() => _game = game);
      final code = await game.exitCode;
      _log('Process exited with code $code.');
      if (mounted) {
        setState(() => _game = null);
      }
    } on ProcessException catch (error) {
      _log('Could not start cargo: ${error.message}');
    } finally {
      if (mounted) {
        setState(() => _starting = false);
      }
    }
  }

  /// Creates the viewport texture once per editor. Without it the game opens
  /// its own window, so failures are logged rather than fatal.
  Future<ViewportTexture?> _ensureViewport() async {
    if (_viewport != null) {
      return _viewport;
    }
    try {
      final viewport = await ViewportTexture.create(
        width: _viewportWidth,
        height: _viewportHeight,
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
                            leadingIcon: const Icon(Icons.arrow_back, size: 16),
                            onPressed: () => Navigator.of(context).pop(),
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
                child: Center(child: _ProjectTitle(widget.project.name)),
              ),
            ],
          ),
        ),
      ),
      body: Column(
        children: [
          _RunToolbar(
            running: _game != null,
            busy: (_starting && _game == null) || _prebuilding,
            onRun: _run,
            onStop: () => _game?.stop(),
            prebuild: widget.prebuild,
          ),
          Expanded(
            child: Center(
              child: _game != null && viewport != null
                  ? AspectRatio(
                      aspectRatio: viewport.width / viewport.height,
                      child: Texture(textureId: viewport.textureId),
                    )
                  : const Text(
                      'Viewport',
                      style: TextStyle(color: Color(0xFF7E8795)),
                    ),
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
    required this.running,
    required this.busy,
    required this.onRun,
    required this.onStop,
    this.prebuild,
  });

  final bool running;
  final bool busy;
  final VoidCallback onRun;
  final VoidCallback onStop;
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
          running
              ? TextButton.icon(
                  onPressed: onStop,
                  icon: const Icon(Icons.stop, size: 18),
                  label: const Text('Stop'),
                )
              : TextButton.icon(
                  onPressed: busy ? null : onRun,
                  icon: const Icon(Icons.play_arrow, size: 18),
                  label: const Text('Run'),
                ),
          if (prebuild case final prebuild?)
            Expanded(
              child: Align(
                alignment: Alignment.centerRight,
                child: BevyPrebuildStatusView(prebuild),
              ),
            ),
        ],
      ),
    );
  }
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
  const _ProjectTitle(this.name);

  final String name;

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
        name,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(fontSize: 12, color: Color(0xFFB3BBC8)),
      ),
    );
  }
}
