import 'dart:io';

import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';

import 'package:editor_ui/entities/project/model/project.dart';
import 'package:editor_ui/features/run_game/model/game_process.dart';

class ProjectEditorPage extends StatefulWidget {
  const ProjectEditorPage({super.key, required this.project});

  final Project project;

  @override
  State<ProjectEditorPage> createState() => _ProjectEditorPageState();
}

const _captionColor = Color(0xFF15171B);

// ponytail: fixed log cap, switch to a ring buffer if trimming shows up in profiles.
const _maxLogLines = 2000;

class _ProjectEditorPageState extends State<ProjectEditorPage> {
  final _logs = <String>[];
  GameProcess? _game;
  bool _starting = false;

  @override
  void initState() {
    super.initState();
    windowManager.setTitleBarStyle(
      TitleBarStyle.hidden,
      windowButtonVisibility: false,
    );
  }

  @override
  void dispose() {
    _game?.stop();
    windowManager.setTitleBarStyle(TitleBarStyle.normal);
    super.dispose();
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

  Future<void> _run() async {
    setState(() {
      _logs.clear();
      _starting = true;
    });
    try {
      final game = await GameProcess.start(widget.project.path, onOutput: _log);
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

  @override
  Widget build(BuildContext context) {
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
            busy: _starting && _game == null,
            onRun: _run,
            onStop: () => _game?.stop(),
          ),
          const Expanded(
            child: Center(
              child: Text(
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
  });

  final bool running;
  final bool busy;
  final VoidCallback onRun;
  final VoidCallback onStop;

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
