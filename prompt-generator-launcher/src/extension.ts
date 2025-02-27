import * as vscode from 'vscode';
import { spawn } from 'child_process';

export function activate(context: vscode.ExtensionContext) {
  let disposable = vscode.commands.registerCommand('promptGenerator.open', (uri: vscode.Uri) => {
    // The uri passed in represents the selected folder.
    const folderPath = uri.fsPath;

    // Specify the path to your Prompt Generator binary.
    // If the binary is in your PATH, you can simply use its name (e.g., "prompt").
    // Otherwise, provide the full absolute path.
    const promptBinary = '/home/andrew-peterson/code/prompt/target/release/prompt'; // or "/absolute/path/to/prompt"

    // Launch the Prompt Generator with the folder as argument.
    const child = spawn(promptBinary, [folderPath], {
      detached: true,
      stdio: 'ignore'
    });
    child.unref();
  });

  context.subscriptions.push(disposable);
}

export function deactivate() { }
