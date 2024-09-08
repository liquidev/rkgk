import { CodeEditor, getLineStart } from "rkgk/code-editor.js";
import { BrushPreview } from "rkgk/brush-preview.js";

const defaultBrush = `
-- This is your brush.
-- Try playing around with the numbers,
-- and see what happens!

stroke 8 #000 (vec 0 0)
`.trim();

export class BrushEditor extends HTMLElement {
    constructor() {
        super();
    }

    connectedCallback() {
        this.classList.add("rkgk-panel");

        this.codeEditor = this.appendChild(
            new CodeEditor([
                {
                    className: "layer-error-squiggles",
                    render: (code, element) => this.#renderErrorSquiggles(code, element),
                },
            ]),
        );
        this.codeEditor.setCode(localStorage.getItem("rkgk.brushEditor.code") ?? defaultBrush);
        this.codeEditor.addEventListener(".codeChanged", (event) => {
            localStorage.setItem("rkgk.brushEditor.code", event.newCode);

            this.dispatchEvent(
                Object.assign(new Event(".codeChanged"), {
                    newCode: event.newCode,
                }),
            );
        });

        this.errorHeader = this.appendChild(document.createElement("h1"));
        this.errorHeader.classList.add("error-header");

        this.errorArea = this.appendChild(document.createElement("pre"));
        this.errorArea.classList.add("errors");
    }

    get code() {
        return this.codeEditor.code;
    }

    resetErrors() {
        this.errorHeader.textContent = "";
        this.errorArea.textContent = "";
    }

    renderHakuResult(phase, result) {
        this.resetErrors();
        this.errorSquiggles = null;

        if (result.status != "error") {
            // We need to request a rebuild if there's no error to remove any squiggles that may be
            // left over from the error state.
            this.codeEditor.renderLayer("layer-error-squiggles");
            return;
        }

        this.errorHeader.textContent = `${phase} failed`;

        if (result.errorKind == "diagnostics") {
            this.codeEditor.rebuildLineMap();
            this.errorSquiggles = this.#computeErrorSquiggles(
                this.codeEditor.lineMap,
                result.diagnostics,
            );
            this.codeEditor.renderLayer("layer-error-squiggles");

            this.errorArea.textContent = result.diagnostics
                .map(
                    (diagnostic) => `${diagnostic.start}..${diagnostic.end}: ${diagnostic.message}`,
                )
                .join("\n");
        } else if (result.errorKind == "plain") {
            this.errorHeader.textContent = result.message;
        } else if (result.errorKind == "exception") {
            // TODO: This should show a stack trace.
            this.errorArea.textContent = `an exception occurred: ${result.message}`;
        } else {
            console.warn(`unknown error kind: ${result.errorKind}`);
            this.errorHeader.textContent = "(unknown error kind)";
        }
    }

    #computeErrorSquiggles(lineMap, diagnostics) {
        // This is an extremely inefficient algorithm.
        // If we had better control of drawing (I'm talking: letter per letter, with a shader!)
        // this could be done lot more efficiently. But alas, HTML giveth, HTML taketh away.

        // The first step is to determine per-line spans.
        // Since we render squiggles by line, it makes the most sense to split the work up to a
        // line by line basis.

        let rawLineSpans = new Map();

        for (let diagnostic of diagnostics) {
            let firstLine = lineMap.lineIndexAt(diagnostic.start);
            let lastLine = lineMap.lineIndexAt(diagnostic.start);
            for (let i = firstLine; i <= lastLine; ++i) {
                let bounds = lineMap.get(i);
                let start = i == firstLine ? diagnostic.start - bounds.start : 0;
                let end = i == lastLine ? diagnostic.end - bounds.start : bounds.end;

                if (!rawLineSpans.has(i)) {
                    rawLineSpans.set(i, []);
                }
                let onThisLine = rawLineSpans.get(i);
                onThisLine.push({ start, end, diagnostic });
            }
        }

        // Once we have the diagnostics subdivided per line, it's time to determine the _boundaries_
        // where diagnostics need to appear.
        // Later we will turn those boundaries into spans, and assign them appropriate classes.

        let segmentedLines = new Map();
        for (let [line, spans] of rawLineSpans) {
            let lineBounds = lineMap.get(line);

            let spanBorderSet = new Set([0]);
            for (let { start, end } of spans) {
                spanBorderSet.add(start);
                spanBorderSet.add(end);
            }
            spanBorderSet.add(lineBounds.end - lineBounds.start);
            let spanBorders = Array.from(spanBorderSet).sort((a, b) => a - b);

            let segments = [];
            let previous = 0;
            for (let i = 1; i < spanBorders.length; ++i) {
                segments.push({ start: previous, end: spanBorders[i], diagnostics: [] });
                previous = spanBorders[i];
            }

            for (let span of spans) {
                for (let segment of segments) {
                    if (segment.start >= span.start && segment.end <= span.end) {
                        segment.diagnostics.push(span.diagnostic);
                    }
                }
            }

            segmentedLines.set(line, segments);
        }

        return segmentedLines;
    }

    #renderErrorSquiggles(lines, element) {
        if (this.errorSquiggles == null) return;

        for (let i = 0; i < lines.lineBounds.length; ++i) {
            let lineBounds = lines.lineBounds[i];
            let lineElement = element.appendChild(document.createElement("span"));
            lineElement.classList.add("line");

            let segments = this.errorSquiggles.get(i);
            if (segments != null) {
                for (let segment of segments) {
                    let text = (lineBounds.substring + " ").substring(segment.start, segment.end);
                    if (segment.diagnostics.length == 0) {
                        lineElement.append(text);
                    } else {
                        let spanElement = lineElement.appendChild(document.createElement("span"));
                        spanElement.classList.add("squiggle", "squiggle-error");
                        spanElement.textContent = text;
                    }
                }
            } else {
                lineElement.textContent = lineBounds.substring;
            }
        }
    }
}

customElements.define("rkgk-brush-editor", BrushEditor);
