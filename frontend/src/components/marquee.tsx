import { useEffect, useRef } from "react";

/// Pixels per second — consistent reading speed regardless of text length.
const MARQUEE_SPEED = 30;

/// Seamlessly looping horizontal text scroll.  Two identical copies of the
/// text sit side-by-side inside a `w-max` container; the animation shifts by
/// exactly −50% (= one copy's width), so the loop reset is invisible.
///
/// Duration is derived from the rendered width of one text copy and written
/// directly to the DOM (bypassing React state) so that ResizeObserver fires
/// never restart the CSS animation mid-scroll.
export default function Marquee({ text }: { text: string }) {
    const spanRef = useRef<HTMLSpanElement>(null);
    const divRef = useRef<HTMLDivElement>(null);

    // Measure one copy's rendered width → set animationDuration directly on
    // the DOM element.  Avoids React re-renders that would restart the
    // animation.  ResizeObserver re-fires when the text content changes.
    useEffect(() => {
        const span = spanRef.current;
        const div = divRef.current;
        if (!span || !div) return;

        let prevDuration = "";
        const measure = () => {
            const next = `${span.offsetWidth / MARQUEE_SPEED}s`;
            if (next !== prevDuration) {
                prevDuration = next;
                div.style.animationDuration = next;
            }
        };

        const observer = new ResizeObserver(measure);
        observer.observe(span);
        measure();
        return () => observer.disconnect();
    }, []);

    const item = `${text}\u2002·\u2002`;

    return (
        <div
            ref={divRef}
            className="flex! overflow-visible! w-max flex-row marquee text-[#bcc0cc] text-sm animate-[marquee_linear_infinite]">
            <span ref={spanRef} className="shrink-0 min-w-auto">{item}</span>
            <span className="shrink-0 min-w-auto">{item}</span>
        </div>
    );
}
