import { useEffect, useMemo, useRef, useState } from "react";
import { Button } from "../../components/button";
import { ErrorNote } from "../../components/onboarding-card";
import { errorText, t, useLanguage } from "../../lib/i18n";
import { clearHighlight, scrollToItem } from "../../lib/scroll-to-item";
import { api, type FileState, type Side } from "../../lib/tauri-commands";
import { useSessionView } from "../../lib/use-session-view";
import { Conversation } from "./conversation";
import { isShown } from "./message-item";
import { ShowSystem } from "./show-system";
import { ViewerHeader } from "./viewer-header";

/** `state`: the transcript's sync state; both copies exist (and can be compared) when one is ahead or they diverged. */
export type ViewedSession = { keyHash: string; sessionId: string; side: Side; state: FileState | null };

const BOTH_COPIES: (FileState | null)[] = ["local_ahead", "remote_ahead", "diverged"];

/** The conversation of one session, shown in place of the project pane. */
export function SessionViewer({ session, onBack, onSide }: { session: ViewedSession; onBack: () => void; onSide: (side: Side) => void }) {
  useLanguage();
  const { view, error, detail, retry } = useSessionView(session.keyHash, session.sessionId, session.side);
  const [showSystem, setShowSystem] = useState(false);
  const [divergeAt, setDivergeAt] = useState<number | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const bothCopies = BOTH_COPIES.includes(session.state);
  // Keyed by position in the whole list, so toggling system events keeps opened blocks open.
  const entries = useMemo(() => view?.items.map((item, index) => ({ item, index })).filter(({ item }) => showSystem || isShown(item)) ?? [], [view, showSystem]);

  // Only a diverged session: comparing means decrypting the whole cloud copy, too costly for a
  // session that is merely ahead (the one Claude is writing right now, often).
  useEffect(() => {
    if (!view || session.state !== "diverged") return;
    api.compareSides(session.keyHash, session.sessionId).then((d) => setDivergeAt(session.side === "local" ? d.local : d.cloud), () => {});
  }, [view, session.state, session.keyHash, session.sessionId, session.side]);
  // The first differing item is often a hidden hook notice: mark the next item that is shown.
  const markerAt = divergeAt === null ? null : (entries.find((e) => e.index >= divergeAt)?.index ?? null);

  useEffect(() => clearHighlight, []);

  const goTo = (item: number, block: number | null = null, query = "") => scrollToItem(box.current, item, block, query);

  return (
    <article className="flex min-w-0 flex-1 flex-col">
      <ViewerHeader
        session={session}
        meta={view?.meta ?? null}
        entries={entries}
        showSystem={showSystem}
        onShowSystem={setShowSystem}
        onSide={bothCopies ? onSide : null}
        divergeAt={markerAt}
        onBack={onBack}
        onGo={goTo}
      />
      {error ? (
        <div className="flex flex-col items-start gap-3 px-8 py-6">
          <ErrorNote message={errorText(error)} />
          <Button variant="secondary" onClick={retry}>
            {t("common.retry")}
          </Button>
        </div>
      ) : !view ? (
        <p className="px-8 py-6 text-body text-ashen">{t("viewer.loading")}</p>
      ) : entries.length === 0 ? (
        <p className="px-8 py-6 text-body text-ashen">{t("viewer.empty")}</p>
      ) : (
        <ShowSystem.Provider value={showSystem}>
          <Conversation entries={entries} detail={detail} box={box} divergeAt={markerAt} />
        </ShowSystem.Provider>
      )}
    </article>
  );
}
