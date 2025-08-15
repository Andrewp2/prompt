import * as vscode from 'vscode';
import { spawn } from 'child_process';

export function activate(context: vscode.ExtensionContext) {
  let disposable = vscode.commands.registerCommand('promptGenerator.open', async (uri: vscode.Uri) => {
    const folderPath = uri.fsPath;

    // 🤖 allow user override via settings; else fallback to 'prompt' on PATH
    const cfg = vscode.workspace.getConfiguration('promptGenerator');
    const configured = cfg.get<string>('binaryPath')?.trim();
    const promptBinary = configured && configured.length > 0 ? configured : 'prompt';

    try {
      const child = spawn(promptBinary, [folderPath], {
        detached: true,
        stdio: 'ignore',
        cwd: folderPath,
      });
      child.unref();
    } catch (err: any) {
      vscode.window.showErrorMessage(
        `Prompt Generator failed to start. Set "promptGenerator.binaryPath" or put 'prompt' in PATH. (${String(err)})`
      );
    }
  });

  context.subscriptions.push(disposable);
}

export function deactivate() { }
