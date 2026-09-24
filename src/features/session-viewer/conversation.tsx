import { useMemo, type RefObject } from "react";
import { t } from "../../lib/i18n";
import { useStickToBottom } from "../../lib/use-stick-to-bottom";
import type { LoadDetail } from "./tool-block";
import { TurnSection, type Entry } from "./turn-section";

type Turn = { prompt: Entry | null; rest: Entry[] };

/** Items before the first prompt form a turn without one. */
function toTurns(entries: Entry[]): Turn[] {
  const turns: Turn[] = [];
  for (const entry of entries) {
    const isPrompt = entry.item.kind === "user";
    if (isPrompt || turns.length === 0) turns.push({ prompt: isPrompt ? entry : null, rest: [] });
    if (!isPrompt) turns[turns.length - 1].rest.push(entry);
  }
  return turns;
}

/** The scrolling conversation: opens at the newest message, prompts stick while their turn is read.
 *  `divergeAt`: index of the first item the other copy of the session does not share. */
export function Conversation({ entries, detail, box, divergeAt }: { entries: Entry[]; detail: LoadDetail; box: RefObject<HTMLDivElement | null>; divergeAt: number | null }) {
  const { far, toEnd } = useStickToBottom(entries.length > 0, box);
  const turns = useMemo(() => toTurns(entries), [entries]);

  return (
    <div className="relative min-h-0 flex-1">
      <div ref={box} className="h-full overflow-y-auto px-8 pb-6">
        <ol className="mx-auto flex max-w-[820px] flex-col gap-6 pt-6">
          {turns.map((turn) => (
            <TurnSection key={(turn.prompt ?? turn.rest[0]).index} prompt={turn.prompt} rest={turn.rest} detail={detail} box={box} divergeAt={divergeAt} />
          ))}
        </ol>
      </div>
      {far && (
        <button type="button" onClick={() => toEnd(true)} className="absolute bottom-5 right-8 rounded-full border border-chalk bg-paper-white px-4 py-2 text-body text-graphite shadow-[0_4px_16px_-6px_rgba(18,18,18,0.3)] hover:text-carbon-ink">
          <span aria-hidden>↓ </span>
          {t("viewer.latest")}
        </button>
      )}
    </div>
  );
}
