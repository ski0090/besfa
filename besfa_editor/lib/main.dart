import 'package:besfa_flutter_plugin/besfa_flutter_plugin.dart';
import 'package:flutter/material.dart';

void main() {
  runApp(const BesfaApp());
}

class BesfaApp extends StatelessWidget {
  const BesfaApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'Besfa',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xFF148F77)),
        useMaterial3: true,
      ),
      home: const EditorShell(),
    );
  }
}

class EditorShell extends StatefulWidget {
  const EditorShell({super.key});

  @override
  State<EditorShell> createState() => _EditorShellState();
}

class _EditorShellState extends State<EditorShell> {
  final BesfaFlutterPlugin _plugin = BesfaFlutterPlugin();
  late final Future<String?> _platformVersion = _plugin.getPlatformVersion();
  late final int _abiVersion = _plugin.abiVersion;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: Column(
          children: [
            const _TopBar(),
            Expanded(
              child: Row(
                children: [
                  const SizedBox(width: 240, child: _SceneTree()),
                  const VerticalDivider(width: 1),
                  Expanded(
                    child: _Viewport(
                      status: _platformVersion,
                      abiVersion: _abiVersion,
                    ),
                  ),
                  const VerticalDivider(width: 1),
                  const SizedBox(width: 280, child: _Inspector()),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _TopBar extends StatelessWidget {
  const _TopBar();

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 52,
      padding: const EdgeInsets.symmetric(horizontal: 16),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surface,
        border: Border(
          bottom: BorderSide(color: Theme.of(context).dividerColor),
        ),
      ),
      child: Row(
        children: [
          const Icon(Icons.two_wheeler, size: 22),
          const SizedBox(width: 10),
          Text('Besfa', style: Theme.of(context).textTheme.titleMedium),
          const Spacer(),
          IconButton(
            tooltip: 'Open project',
            onPressed: () {},
            icon: const Icon(Icons.folder_open),
          ),
          IconButton(
            tooltip: 'Run preview',
            onPressed: () {},
            icon: const Icon(Icons.play_arrow),
          ),
          IconButton(
            tooltip: 'Reload runtime',
            onPressed: () {},
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
    );
  }
}

class _SceneTree extends StatelessWidget {
  const _SceneTree();

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.all(12),
      children: const [
        _PanelTitle('Scene'),
        _TreeItem(icon: Icons.public, label: 'World'),
        _TreeItem(icon: Icons.videocam, label: 'Camera3d', indent: 16),
        _TreeItem(icon: Icons.light_mode, label: 'Key Light', indent: 16),
        _TreeItem(icon: Icons.grid_on, label: 'Ground', indent: 16),
      ],
    );
  }
}

class _Viewport extends StatelessWidget {
  const _Viewport({required this.status, required this.abiVersion});

  final Future<String?> status;
  final int abiVersion;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xFFF4F1EA),
      child: Stack(
        children: [
          Center(
            child: Container(
              width: 520,
              height: 320,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: const Color(0xFF202124),
                borderRadius: BorderRadius.circular(6),
              ),
              child: const Text(
                'Preview surface',
                style: TextStyle(color: Colors.white70),
              ),
            ),
          ),
          Positioned(
            left: 16,
            bottom: 16,
            child: FutureBuilder<String?>(
              future: status,
              builder: (context, snapshot) {
                final platform = snapshot.data ?? 'platform pending';
                return Text(
                  '$platform | Rust ABI $abiVersion',
                  style: Theme.of(context).textTheme.bodySmall,
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}

class _Inspector extends StatefulWidget {
  const _Inspector();

  @override
  State<_Inspector> createState() => _InspectorState();
}

class _InspectorState extends State<_Inspector> {
  final TextEditingController _name = TextEditingController(text: 'Camera3d');

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.all(12),
      children: [
        const _PanelTitle('Inspector'),
        TextField(
          controller: _name,
          decoration: const InputDecoration(
            labelText: 'Name',
            border: OutlineInputBorder(),
          ),
        ),
        const SizedBox(height: 12),
        const _PropertyRow(label: 'Transform', value: '0, 4, 8'),
        const _PropertyRow(label: 'Projection', value: 'Perspective'),
        const _PropertyRow(label: 'Runtime', value: 'Bevy 0.18'),
      ],
    );
  }
}

class _PanelTitle extends StatelessWidget {
  const _PanelTitle(this.label);

  final String label;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Text(label, style: Theme.of(context).textTheme.labelLarge),
    );
  }
}

class _TreeItem extends StatelessWidget {
  const _TreeItem({required this.icon, required this.label, this.indent = 0});

  final IconData icon;
  final String label;
  final double indent;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(left: indent),
      child: ListTile(
        dense: true,
        leading: Icon(icon, size: 18),
        title: Text(label),
        contentPadding: EdgeInsets.zero,
      ),
    );
  }
}

class _PropertyRow extends StatelessWidget {
  const _PropertyRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(
        children: [
          SizedBox(
            width: 92,
            child: Text(label, style: Theme.of(context).textTheme.labelMedium),
          ),
          Expanded(child: Text(value)),
        ],
      ),
    );
  }
}
