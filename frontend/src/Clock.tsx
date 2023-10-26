import classNames from "classnames";
import "./Clock.scss";

export type Mode = "mini" | "big" | "miniPlaceholder";

export function Clock({
  duration,
  mode = "big",
}: {
  duration: number;
  mode: Mode;
}) {
  return (
    <div className={`Clock ${mode}`}>
        <span>{(duration / 1000).toFixed(3)}</span>
    </div>
  );
}
