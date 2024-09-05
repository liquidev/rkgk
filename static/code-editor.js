export class CodeEditor extends HTMLElement {
    constructor() {
        super();
    }

    connectedCallback() {
        this.indentWidth = 2;

        this.textArea = this.appendChild(document.createElement("textarea"));
        this.textArea.spellcheck = false;
        this.textArea.rows = 1;

        this.keyShortcuts = {
            Enter: () => this.enter(),

            Tab: () => this.tab(),
            "Shift-Tab": () => this.decreaseIndent(),
            "Ctrl-[": () => this.decreaseIndent(),
            "Ctrl-]": () => this.increaseIndent(),

            "Ctrl-z": () => this.undo(),
            "Ctrl-Shift-z": () => this.redo(),
            "Ctrl-y": () => this.redo(),
        };

        this.undoMergeTimeout = 300;
        this.undoHistory = [];
        this.undoHistoryTop = 0;

        this.#textAreaAutoSizingBehaviour();
        this.#keyShortcutBehaviours();
    }

    get code() {
        return this.textArea.value;
    }

    #sendCodeChanged() {
        this.dispatchEvent(
            Object.assign(new Event(".codeChanged"), {
                newCode: this.code,
            }),
        );
    }

    setCode(value) {
        this.textArea.value = value;
        this.#resizeTextArea();
        this.#sendCodeChanged();
    }

    #textAreaAutoSizingBehaviour() {
        this.textArea.addEventListener("input", () => {
            this.#resizeTextArea();
            this.#sendCodeChanged();
        });
        this.#resizeTextArea();
        document.fonts.addEventListener("loadingdone", () => this.#resizeTextArea());
        new ResizeObserver(() => this.#resizeTextArea()).observe(this.textArea);
    }

    getSelection() {
        // NOTE: We only support one selection, because multiple selections are only
        // supported by Firefox.

        if (document.activeElement != this.textArea) return null;

        if (this.textArea.selectionDirection == "forward") {
            return new Selection(this.textArea.selectionStart, this.textArea.selectionEnd);
        } else {
            return new Selection(this.textArea.selectionEnd, this.textArea.selectionStart);
        }
    }

    setSelection(selection) {
        this.textArea.selectionDirection =
            selection.anchor < selection.cursor ? "forward" : "backward";
        this.textArea.selectionStart = selection.start;
        this.textArea.selectionEnd = selection.end;
    }

    #resizeTextArea() {
        this.textArea.style.height = "";
        this.textArea.style.height = `${this.textArea.scrollHeight}px`;
    }

    #keyShortcutBehaviours() {
        this.textArea.addEventListener("keydown", (event) => {
            let keyComponents = [];
            if (event.ctrlKey) keyComponents.push("Ctrl");
            if (event.altKey) keyComponents.push("Alt");
            if (event.shiftKey) keyComponents.push("Shift");
            keyComponents.push(event.key);

            let keyChord = keyComponents.join("-");

            let shortcut = this.keyShortcuts[keyChord];
            if (shortcut != null) {
                shortcut();
                event.preventDefault();
            }
        });

        this.textArea.addEventListener("beforeinput", () => {
            this.pushHistory({ allowMerge: true });
        });
    }

    replace(selection, text) {
        let left = this.code.substring(0, selection.start);
        let right = this.code.substring(selection.end);
        this.setCode(left + text + right);
    }

    pushHistory({ allowMerge }) {
        this.undoHistory.splice(this.undoHistoryTop);

        if (allowMerge && this.undoHistory.length > 1) {
            let last = this.undoHistory[this.undoHistory.length - 1];
            let elapsed = performance.now() - last.time;
            if (elapsed < this.undoMergeTimeout) {
                last.time = performance.now();
                last.code = this.code;
                last.selection = this.getSelection();
                return;
            }
        }

        this.undoHistory.push({
            time: performance.now(),
            code: this.code,
            selection: this.getSelection(),
        });
        this.undoHistoryTop += 1;
    }

    popHistory() {
        let entry = this.undoHistory[this.undoHistoryTop - 1];
        if (entry == null) return null;

        this.undoHistoryTop -= 1;
        this.setCode(entry.code);
        this.setSelection(entry.selection);

        return entry;
    }

    insertTab(selection) {
        let positionInLine = getPositionInLine(this.code, selection.cursor);
        let positionInTab = positionInLine % this.indentWidth;
        let spaceCount = this.indentWidth - positionInTab;
        this.replace(selection, " ".repeat(spaceCount));
        selection.advance(this.code, spaceCount);
    }

    indent(selection) {
        let start = getLineStart(this.code, selection.start);
        let end = getLineEnd(this.code, selection.end);

        let indent = " ".repeat(this.indentWidth);
        let indented = this.code.substring(start, end).split(/^/m);
        for (let i = 0; i < indented.length; ++i) {
            indented[i] = indent + indented[i];
        }
        this.replace(new Selection(start, end), indented.join(""));

        if (selection.anchor < selection.cursor) {
            selection.anchor += this.indentWidth;
            selection.cursor += this.indentWidth * indented.length;
        } else {
            selection.anchor += this.indentWidth * indented.length;
            selection.cursor += this.indentWidth;
        }
    }

    unindent(selection) {
        let start = getLineStart(this.code, selection.start);
        let end = getLineEnd(this.code, selection.end);

        let indent = " ".repeat(this.indentWidth);
        let unindented = this.code.substring(start, end).split(/^/m);
        for (let i = 0; i < unindented.length; ++i) {
            if (unindented[i].startsWith(indent)) {
                unindented[i] = unindented[i].substring(this.indentWidth);
            }
        }
        this.replace(new Selection(start, end), unindented.join(""));

        if (selection.anchor < selection.cursor) {
            selection.anchor -= this.indentWidth;
            selection.cursor -= this.indentWidth * unindented.length;
        } else {
            selection.anchor -= this.indentWidth * unindented.length;
            selection.cursor -= this.indentWidth;
        }
    }

    enter() {
        this.pushHistory({ allowMerge: false });

        let selection = this.getSelection();

        let start = getLineStart(this.code, selection.start);
        let indent = countSpaces(this.code, start);

        let newLine = "\n" + " ".repeat(indent);
        this.replace(selection, newLine);
        selection.set(this.code, selection.start + newLine.length);

        this.setSelection(selection);
    }

    tab() {
        this.pushHistory({ allowMerge: false });

        let selection = this.getSelection();
        if (selection == null) return;

        if (selection.anchor == selection.cursor) {
            this.insertTab(selection);
        } else {
            this.indent(selection);
        }

        this.setSelection(selection);
    }

    increaseIndent() {
        this.pushHistory({ allowMerge: false });

        let selection = this.getSelection();
        this.indent(selection);
        this.setSelection(selection);
    }

    decreaseIndent() {
        this.pushHistory({ allowMerge: false });

        let selection = this.getSelection();
        this.unindent(selection);
        this.setSelection(selection);
    }

    undo() {
        let code = this.code;
        let popped = this.popHistory();
        if (popped != null && popped.code == code) {
            this.popHistory();
        }
    }

    redo() {
        let entry = this.undoHistory[this.undoHistoryTop];
        if (entry == null) return;

        this.undoHistoryTop += 1;
        this.setCode(entry.code);
        this.setSelection(entry.selection);
    }
}

customElements.define("rkgk-code-editor", CodeEditor);

class Selection {
    constructor(anchor, cursor) {
        this.anchor = anchor;
        this.cursor = cursor;
    }

    get start() {
        return Math.min(this.anchor, this.cursor);
    }

    get end() {
        return Math.max(this.anchor, this.cursor);
    }

    clampCursor(text) {
        this.cursor = Math.max(0, Math.min(this.cursor, text.length));
    }

    set(text, n) {
        this.cursor = n;
        this.clampCursor(text);
        this.anchor = this.cursor;
    }

    advance(text, n) {
        this.cursor += n;
        this.clampCursor(text);
        this.anchor = this.cursor;
    }
}

function getLineStart(string, position) {
    do {
        --position;
    } while (string.charAt(position) != "\n" && position > 0);
    if (string.charAt(position) == "\n") ++position;
    return position;
}

function getLineEnd(string, position) {
    while (string.charAt(position) != "\n" && position < string.length) ++position;
    return position + 1;
}

function getPositionInLine(string, position) {
    return position - getLineStart(string, position);
}

function countSpaces(string, position) {
    let count = 0;
    while (string.charAt(position) == " ") {
        ++count;
        ++position;
    }
    return count;
}
