import 'dart:io';

/// Moves [directory] to the Recycle Bin. Returns false if it failed.
Future<bool> moveToRecycleBin(String directory) async {
  try {
    // The path goes through an env var so quotes in it cannot break the script.
    final result = await Process.run(
      'powershell',
      [
        '-NoProfile',
        '-NonInteractive',
        '-Command',
        r"$ErrorActionPreference = 'Stop'; "
            'Add-Type -AssemblyName Microsoft.VisualBasic; '
            r'[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory('
            r"$env:BESFA_DELETE_PATH, 'OnlyErrorDialogs', 'SendToRecycleBin')",
      ],
      environment: {'BESFA_DELETE_PATH': directory},
    );
    return result.exitCode == 0;
  } on ProcessException {
    return false;
  }
}
