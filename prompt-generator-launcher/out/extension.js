"use strict";
var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = require("vscode");
const fs = require("fs");
const child_process_1 = require("child_process");
// 🤖 Absolute path to your Prompt binary (change if needed)
const HARDCODED_PROMPT_BINARY = '/home/andrew-peterson/code/prompt/target/release/prompt';
// 🤖 Simple output channel so we can see what binary/path we’re using
const out = vscode.window.createOutputChannel('Prompt Generator Launcher');
// 🔹 Always resolve to the WORKSPACE ROOT (project root), not the active file’s folder
function getWorkspaceRootFsPath(uriHint) {
    return __awaiter(this, void 0, void 0, function* () {
        var _a;
        const folders = vscode.workspace.workspaceFolders;
        if (!folders || folders.length === 0) {
            vscode.window.showErrorMessage('No workspace folder is open.');
            return undefined;
        }
        // If we were invoked with a URI (e.g., context menu), pick the workspace containing it.
        if (uriHint) {
            const ws = vscode.workspace.getWorkspaceFolder(uriHint);
            if (ws)
                return ws.uri.fsPath;
        }
        // Single-root workspace: just use it.
        if (folders.length === 1) {
            return folders[0].uri.fsPath;
        }
        // Multi-root: prefer the root that contains the active editor if available.
        const activeUri = (_a = vscode.window.activeTextEditor) === null || _a === void 0 ? void 0 : _a.document.uri;
        if (activeUri) {
            const ws = vscode.workspace.getWorkspaceFolder(activeUri);
            if (ws)
                return ws.uri.fsPath;
        }
        // Still ambiguous: ask the user which workspace ROOT to use.
        const pick = yield vscode.window.showQuickPick(folders.map(f => ({ label: f.name, description: f.uri.fsPath })), { placeHolder: 'Select a workspace root to open in Prompt Generator' });
        if (!pick)
            return undefined;
        const chosen = folders.find(f => f.name === pick.label);
        return chosen === null || chosen === void 0 ? void 0 : chosen.uri.fsPath;
    });
}
// 🤖 Spawn helper using the hardcoded path, with safety + logs
function launchPrompt(folderPath) {
    out.appendLine(`[launcher] launching with binary: ${HARDCODED_PROMPT_BINARY}`);
    out.appendLine(`[launcher] folder(root): ${folderPath}`);
    if (!fs.existsSync(HARDCODED_PROMPT_BINARY)) {
        vscode.window.showErrorMessage(`Prompt binary not found at:\n${HARDCODED_PROMPT_BINARY}\n\nUpdate HARDCODED_PROMPT_BINARY in extension.ts.`);
        return;
    }
    try {
        const child = (0, child_process_1.spawn)(HARDCODED_PROMPT_BINARY, [folderPath], {
            detached: true,
            stdio: 'ignore',
            cwd: folderPath,
        });
        child.unref();
    }
    catch (err) {
        vscode.window.showErrorMessage(`Failed to start Prompt Generator at ${HARDCODED_PROMPT_BINARY}\n${String(err)}\n\n` +
            `Ensure it is executable (chmod +x) and reachable by Windsurf.`);
    }
}
function activate(context) {
    // Make the button very visible: left side, high priority
    const item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 1000);
    item.name = 'Prompt Generator';
    item.text = '$(rocket) Prompt';
    item.command = 'promptGenerator.openHere';
    item.tooltip = 'Open Prompt Generator (workspace root)';
    item.show();
    context.subscriptions.push(item, out);
    out.appendLine(`[launcher] activated; using HARDCODED_PROMPT_BINARY=${HARDCODED_PROMPT_BINARY}`);
    // Context menu command: open at the ROOT of the workspace containing the clicked item
    const openCmd = vscode.commands.registerCommand('promptGenerator.open', (uri) => __awaiter(this, void 0, void 0, function* () {
        const fsPath = yield getWorkspaceRootFsPath(uri);
        if (!fsPath)
            return;
        launchPrompt(fsPath);
    }));
    // Status bar / explorer title / hotkey: open at current workspace ROOT
    const openHereCmd = vscode.commands.registerCommand('promptGenerator.openHere', () => __awaiter(this, void 0, void 0, function* () {
        const fsPath = yield getWorkspaceRootFsPath();
        if (!fsPath)
            return;
        launchPrompt(fsPath);
    }));
    context.subscriptions.push(openCmd, openHereCmd);
}
function deactivate() { }
//# sourceMappingURL=extension.js.map