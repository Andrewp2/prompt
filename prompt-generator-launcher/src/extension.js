"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
var vscode = require("vscode");
var child_process_1 = require("child_process");
function activate(context) {
    var disposable = vscode.commands.registerCommand('promptGenerator.open', function (uri) {
        // The uri passed in represents the selected folder.
        var folderPath = uri.fsPath;
        // Specify the path to your Prompt Generator binary.
        // If the binary is in your PATH, you can simply use its name (e.g., "prompt").
        // Otherwise, provide the full absolute path.
        var promptBinary = 'prompt'; // or "/absolute/path/to/prompt"
        // Launch the Prompt Generator with the folder as argument.
        var child = (0, child_process_1.spawn)(promptBinary, [folderPath], {
            detached: true,
            stdio: 'ignore'
        });
        child.unref();
    });
    context.subscriptions.push(disposable);
}
function deactivate() { }
