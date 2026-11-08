import { useEffect, useState } from "react";
import { Marquee } from "../components/Marquee";
import { Note } from "../components/Icons";
import type { Layout } from "../lib/layout";
import type { Playback } from "../lib/ipc";

/** Crossfading artwork: keeps the previous image underneath until the new one has loaded. */
export function Artwork({ url, className }: { url: string | null; className?: string }) {
  const [shown, setShown] = useState<string | null>(url);
  const [prev, setPrev] = useState<string | null>(null);
  useEffect(() => {
    if (url === shown) return;
    if (!url) {
      setShown(null);
      return;
    }
    const img = new Image();
    img.src = url;
    img.decode().catch(() => {}).finally(() => {
      setPrev(shown);
      setShown(url);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url]);

  return (
    <div className={`art ${className ?? ""}`}>
      {prev && <img key={`p-${prev}`} src={prev} className="art-img" alt="" />}
      {shown ? (
        <img key={shown} src={shown} className="art-img art-in" alt="" draggable={false} />
      ) : (
        <div className="art-placeholder"><Note width={36} height={36} /></div>
      )}
    </div>
  );
}

export function TrackInfo({ p }: { p: Playback }) {
  return (
    <div className="trackinfo">
      <Marquee className="title" text={p.title || "Nothing playing"} />
      <Marquee className="artist" text={p.artists.join(", ")} />
    </div>
  );
}

export function ArtworkView({ p, layout }: { p: Playback; layout: Layout }) {
  if (p.kind === "ad") {
    return <div className="center-msg"><Note width={28} height={28} /><p>Advertisement</p></div>;
  }
  return (
    <div className={`artview artview-${layout}`}>
      <Artwork url={p.artUrl} className={p.isPlaying ? "" : "paused"} />
      {layout !== "compact" && <TrackInfo p={p} />}
      {layout === "compact" && <div className="art-overlay"><TrackInfo p={p} /></div>}
      {p.kind === "episode" && layout === "tall" && p.description && (
        <p className="show-notes">{p.description}</p>
      )}
    </div>
  );
}
