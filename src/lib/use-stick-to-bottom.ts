import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";

/** Within this distance of the end, the box counts as "at the end" and follows new height. */
const NEAR_PX = 80;
/** Past this distance, offer the jump back to the end. */
const FAR_PX = 600;

/** A scroll box that opens at its end and stays there until the user scrolls away. Items below
 *  keep changing height as they render (`content-visibility`), so "the end" moves for a while. */
export function useStickToBottom(ready: boolean, box: RefObject<HTMLDivElement | null>) {
  const pinned = useRef(true);
  const [far, setFar] = useState(false);

  const toEnd = useCallback((smooth = false) => {
    const el = box.current;
    if (!el) return;
    pinned.current = true;
    el.scrollTo({ top: el.scrollHeight, behavior: smooth ? "smooth" : "auto" });
  }, [box]);

  useLayoutEffect(() => {
    if (ready) toEnd();
  }, [ready, toEnd]);

  useEffect(() => {
    const el = box.current;
    const list = el?.firstElementChild;
    if (!ready || !el || !list) return;
    const onScroll = () => {
      const gap = el.scrollHeight - el.scrollTop - el.clientHeight;
      pinned.current = gap < NEAR_PX;
      setFar(gap > FAR_PX);
    };
    const follow = new ResizeObserver(() => {
      if (pinned.current) el.scrollTop = el.scrollHeight;
    });
    follow.observe(list);
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      follow.disconnect();
      el.removeEventListener("scroll", onScroll);
    };
  }, [ready, box]);

  return { far, toEnd };
}
