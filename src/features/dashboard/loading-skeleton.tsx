/** Placeholders shaped like the real sidebar and detail pane, shown until the first list arrives. */
const BAR = "rounded-control bg-chalk animate-pulse motion-reduce:animate-none";

export function SidebarSkeleton() {
  return (
    <div className="flex flex-col gap-2 px-3 pt-4" aria-hidden="true">
      <span className={`${BAR} mb-2 h-3 w-24`} />
      {[70, 55, 80, 60, 45, 65].map((width, i) => (
        <span key={i} className={`${BAR} h-6`} style={{ width: `${width}%` }} />
      ))}
    </div>
  );
}

export function DetailSkeleton() {
  return (
    <div className="flex flex-1 flex-col gap-8 p-8" aria-hidden="true">
      <div className="flex flex-col gap-3">
        <span className={`${BAR} h-8 w-64`} />
        <span className={`${BAR} h-4 w-80`} />
        <span className={`${BAR} mt-2 h-5 w-40`} />
      </div>
      <div className="flex flex-col gap-4">
        {[90, 75, 85, 60].map((width, i) => (
          <span key={i} className={`${BAR} h-10`} style={{ width: `${width}%` }} />
        ))}
      </div>
    </div>
  );
}
