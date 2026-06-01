import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// Collapsible bottom console for runtime log events.
class RuntimeLogPanel extends StatefulWidget {
  const RuntimeLogPanel({required this.logs, super.key});

  /// Runtime log entries in display order.
  final List<RuntimeLogEntry> logs;

  @override
  State<RuntimeLogPanel> createState() => _RuntimeLogPanelState();
}

class _RuntimeLogPanelState extends State<RuntimeLogPanel> {
  static const double _collapsedHeight = 36;
  static const double _expandedHeight = 180;
  static const double _headerHeight = 34;

  bool _isExpanded = false;

  @override
  Widget build(BuildContext context) {
    final latestLog = widget.logs.isEmpty ? null : widget.logs.last;
    final colorScheme = Theme.of(context).colorScheme;

    return AnimatedContainer(
      duration: const Duration(milliseconds: 140),
      curve: Curves.easeOut,
      height: _isExpanded ? _expandedHeight : _collapsedHeight,
      decoration: BoxDecoration(
        color: colorScheme.surface,
        border: Border(top: BorderSide(color: Theme.of(context).dividerColor)),
      ),
      child: Column(
        children: [
          SizedBox(
            height: _headerHeight,
            child: Row(
              children: [
                IconButton(
                  tooltip: _isExpanded ? 'Collapse logs' : 'Expand logs',
                  constraints: const BoxConstraints.tightFor(
                    width: 34,
                    height: 34,
                  ),
                  padding: EdgeInsets.zero,
                  onPressed: () {
                    setState(() {
                      _isExpanded = !_isExpanded;
                    });
                  },
                  icon: Icon(
                    _isExpanded
                        ? Icons.keyboard_arrow_down
                        : Icons.keyboard_arrow_up,
                  ),
                ),
                Icon(
                  Icons.terminal,
                  size: 16,
                  color: colorScheme.onSurfaceVariant,
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    latestLog == null
                        ? 'No runtime logs'
                        : '[${latestLog.level.toUpperCase()}] ${latestLog.message}',
                    overflow: TextOverflow.ellipsis,
                    maxLines: 1,
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                ),
                IconButton(
                  tooltip: 'Copy logs',
                  constraints: const BoxConstraints.tightFor(
                    width: 34,
                    height: 34,
                  ),
                  padding: EdgeInsets.zero,
                  onPressed: widget.logs.isEmpty ? null : _copyLogs,
                  icon: const Icon(Icons.copy),
                ),
              ],
            ),
          ),
          if (_isExpanded) ...[
            Divider(height: 1, color: Theme.of(context).dividerColor),
            Expanded(
              child: widget.logs.isEmpty
                  ? Center(
                      child: Text(
                        'No runtime logs',
                        style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: colorScheme.onSurfaceVariant,
                        ),
                      ),
                    )
                  : ListView.builder(
                      padding: const EdgeInsets.symmetric(vertical: 6),
                      itemCount: widget.logs.length,
                      itemBuilder: (context, index) {
                        return _LogLine(entry: widget.logs[index]);
                      },
                    ),
            ),
          ],
        ],
      ),
    );
  }

  Future<void> _copyLogs() async {
    final text = widget.logs
        .map((log) => '[${log.level.toUpperCase()}] ${log.message}')
        .join('\n');
    await Clipboard.setData(ClipboardData(text: text));
  }
}

class _LogLine extends StatelessWidget {
  const _LogLine({required this.entry});

  final RuntimeLogEntry entry;

  @override
  Widget build(BuildContext context) {
    final levelColor = _levelColor(Theme.of(context).colorScheme);

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 3),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 56,
            child: Text(
              entry.level.toUpperCase(),
              overflow: TextOverflow.ellipsis,
              maxLines: 1,
              style: Theme.of(context).textTheme.labelSmall?.copyWith(
                color: levelColor,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: SelectableText(
              entry.message,
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ),
        ],
      ),
    );
  }

  Color _levelColor(ColorScheme colorScheme) {
    return switch (entry.level.toLowerCase()) {
      'error' => colorScheme.error,
      'warn' || 'warning' => colorScheme.tertiary,
      'debug' => colorScheme.outline,
      _ => colorScheme.primary,
    };
  }
}
