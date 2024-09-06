export class CodeEditor extends HTMLElement {
    constructor(layers) {
        super();

        this.layers = layers;
    }

    connectedCallback() {
        this.indentWidth = 2;

        this.layerGutter = this.appendChild(document.createElement("pre"));
        this.layerGutter.classList.add("layer", "layer-gutter");

        this.userLayers = [];
        for (let layer of this.layers) {
            let element = this.appendChild(document.createElement("pre"));
            element.classList.add("layer", layer.className);
            this.userLayers.push({
                def: layer,
                element,
            });
        }

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

        this.#renderLayers();
    }

    get code() {
        return this.textArea.value;
    }

    #codeChanged() {
        this.#resizeTextArea();
        this.#renderLayers();
        this.dispatchEvent(
            Object.assign(new Event(".codeChanged"), {
                newCode: this.code,
            }),
        );
    }

    setCode(value) {
        this.textArea.value = value;
        this.#codeChanged();
    }

    // Resizing the text area

    #textAreaAutoSizingBehaviour() {
        this.textArea.addEventListener("input", () => {
            this.#codeChanged();
        });
        this.#resizeTextArea();
        document.fonts.addEventListener("loadingdone", () => this.#resizeTextArea());
        new ResizeObserver(() => this.#resizeTextArea()).observe(this.textArea);
    }

    #resizeTextArea() {
        this.textArea.style.height = "";
        this.textArea.style.height = `${this.textArea.scrollHeight}px`;
    }

    // Layers

    rebuildLineMap() {
        this.lineMap = new LineMap(this.code);
    }

    #renderLayers() {
        this.rebuildLineMap();

        this.#renderGutter();
        for (let userLayer of this.userLayers) {
            userLayer.element.replaceChildren();
            userLayer.def.render(this.lineMap, userLayer.element);
        }
    }

    #renderGutter() {
        this.layerGutter.replaceChildren();

        for (let lineBounds of this.lineMap.lineBounds) {
            let lineElement = this.layerGutter.appendChild(document.createElement("span"));
            lineElement.classList.add("line");
            lineElement.textContent = lineBounds.substring;
        }
    }

    renderLayer(className) {
        for (let userLayer of this.userLayers) {
            if (userLayer.def.className == className) {
                userLayer.element.replaceChildren();
                userLayer.def.render(this.lineMap, userLayer.element);
            }
        }
    }

    // Text editing

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

export class Selection {
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

export function getLineStart(string, position) {
    do {
        --position;
    } while (string.charAt(position) != "\n" && position > 0);
    if (string.charAt(position) == "\n") ++position;
    return position;
}

export function getLineEnd(string, position) {
    while (string.charAt(position) != "\n" && position < string.length) ++position;
    return position + 1;
}

export function getPositionInLine(string, position) {
    return position - getLineStart(string, position);
}

export function countSpaces(string, position) {
    let count = 0;
    while (string.charAt(position) == " ") {
        ++count;
        ++position;
    }
    return count;
}

export class LineMap {
    constructor(string) {
        // This simplifies the algorithm below a bit.
        string += "\n";

        this.string = string;
        this.lineBounds = [];

        let start = 0;
        for (let i = 0; i < string.length; ++i) {
            if (string.charAt(i) == "\n") {
                let substring = string.substring(start, i);
                this.lineBounds.push({ start, end: i, substring });
                start = i + 1;
            }
        }
        if (start < string.length) {
            this.lineBounds.push({ start, end: string.length, substring: string.substring(start) });
        }
    }

    get(lineIndex) {
        return this.lineBounds[lineIndex];
    }

    lineIndexAt(position) {
        // Ported from the Rust 1.81 standard library binary search.
        // I was too lazy to come up with the algorithm myself. Sorry to disappoint.

        let size = this.lineBounds.length;
        let left = 0;
        let right = size;
        while (left < right) {
            let mid = (left + size / 2) | 0;

            let isLess = this.lineBounds[mid].start < position;
            let isEqual = this.lineBounds[mid].start == position;
            left = isLess && !isEqual ? mid + 1 : left;
            right = !isLess && !isEqual ? mid : right;
            if (isEqual) {
                return mid;
            }

            size = right - left;
        }

        return left - 1;
    }
}
