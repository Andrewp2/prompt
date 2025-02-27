"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = require("vscode");
const child_process_1 = require("child_process");
function activate(context) {
    let disposable = vscode.commands.registerCommand('promptGenerator.open', (uri) => {
        // The uri passed in represents the selected folder.
        const folderPath = uri.fsPath;
        // Specify the path to your Prompt Generator binary.
        // If the binary is in your PATH, you can simply use its name (e.g., "prompt").
        // Otherwise, provide the full absolute path.
        const promptBinary = '/home/andrew-peterson/code/prompt/target/release/prompt'; // or "/absolute/path/to/prompt"
        // Launch the Prompt Generator with the folder as argument.
        const child = (0, child_process_1.spawn)(promptBinary, [folderPath], {
            detached: true,
            stdio: 'ignore',
            cwd: folderPath
        });
        child.unref();
    });
    context.subscriptions.push(disposable);
}
function deactivate() { }
//# sourceMappingURL=extension.js.map