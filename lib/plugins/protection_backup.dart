import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'host_notices.dart';

class ProtectionBackup extends StatefulWidget {
  const ProtectionBackup({
    super.key,
    required this.onBackup,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.chooseDestination,
    this.onSnapshot,
    this.chooseSnapshotDestination,
  });
  final Future<void> Function(String) onBackup;
  final Future<void> Function(String)? onSnapshot;
  final Future<String?> Function()? chooseSnapshotDestination;
  final Future<String?> Function()? chooseDestination;
  final Color ink, muted, line;
  final BorderRadius radius;
  @override
  State<ProtectionBackup> createState() => _ProtectionBackupState();
}

class _ProtectionBackupState extends State<ProtectionBackup> {
  bool _busy = false;
  String Function(AppLocalizations)? _message;
  bool _failed = false;
  Future<void> _save({bool snapshot = false}) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _message = null;
      _failed = false;
    });
    try {
      final String? path;
      final choose = snapshot
          ? widget.chooseSnapshotDestination
          : widget.chooseDestination;
      if (choose != null) {
        path = await choose();
      } else {
        final stamp = DateTime.now()
            .toIso8601String()
            .replaceAll(':', '-')
            .replaceAll('.', '-');
        path = (await getSaveLocation(
          suggestedName: snapshot
              ? 'morrow-library-$stamp.morrowbackup'
              : 'morrow-protection-$stamp.backup',
          acceptedTypeGroups: [
            XTypeGroup(
              label: snapshot
                  ? L10n.of(context).pluginsBackupLibraryType
                  : L10n.of(context).pluginsProtectionFileType,
              extensions: [snapshot ? 'morrowbackup' : 'backup'],
            ),
          ],
        ))?.path;
      }
      if (path == null || !mounted) return;
      await (snapshot ? widget.onSnapshot! : widget.onBackup)(path);
      if (mounted) {
        setState(() {
          _message = (l) =>
              snapshot ? l.pluginsSnapshotSaved : l.pluginsProtectionSaved;
        });
      }
    } catch (error) {
      if (mounted) {
        setState(() {
          _failed = true;
          final detail = error is StateError ? error.message.toString() : '';
          _message = (l) =>
              hostStorageNotice(l, detail) ?? l.pluginsBackupUnknown;
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(19),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(Icons.key_rounded, size: 16, color: widget.ink),
            const SizedBox(width: 8),
            Text(
              L10n.of(context).pluginsProtection,
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: widget.ink,
              ),
            ),
          ],
        ),
        const SizedBox(height: 12),
        Text(
          L10n.of(context).pluginsProtectionDetails,
          style: TextStyle(fontSize: 11, height: 1.6, color: widget.muted),
        ),
        const SizedBox(height: 12),
        if (widget.onSnapshot != null) ...[
          Text(
            L10n.of(context).pluginsSnapshotDetails,
            style: TextStyle(fontSize: 11, height: 1.6, color: widget.muted),
          ),
          const SizedBox(height: 12),
          OutlinedButton.icon(
            key: const ValueKey('backup-snapshot'),
            onPressed: _busy ? null : () => _save(snapshot: true),
            icon: const Icon(Icons.inventory_2_outlined, size: 16),
            label: Text(
              L10n.of(context).pluginsBackupLibrary,
              style: TextStyle(fontSize: 11),
            ),
            style: OutlinedButton.styleFrom(
              foregroundColor: widget.ink,
              minimumSize: const Size.fromHeight(38),
              side: BorderSide(color: widget.line),
              shape: RoundedRectangleBorder(borderRadius: widget.radius),
            ),
          ),
          const SizedBox(height: 8),
        ],
        OutlinedButton.icon(
          key: const ValueKey('backup-protection'),
          onPressed: _busy ? null : _save,
          icon: const Icon(Icons.save_alt_rounded, size: 16),
          label: Text(
            _busy
                ? L10n.of(context).pluginsBackingUp
                : L10n.of(context).pluginsBackupProtection,
            style: const TextStyle(fontSize: 11),
          ),
          style: OutlinedButton.styleFrom(
            foregroundColor: widget.ink,
            minimumSize: const Size.fromHeight(38),
            side: BorderSide(color: widget.line),
            shape: RoundedRectangleBorder(borderRadius: widget.radius),
          ),
        ),
        if (_message != null) ...[
          const SizedBox(height: 10),
          Text(
            _message!(L10n.of(context)),
            style: TextStyle(
              fontSize: 11,
              height: 1.6,
              color: _failed
                  ? Theme.of(context).colorScheme.error
                  : widget.muted,
            ),
          ),
        ],
      ],
    ),
  );
}
