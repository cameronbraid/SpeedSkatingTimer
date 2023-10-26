import useRequestAnimationFrame from "use-request-animation-frame";
import { useState } from "react";
import { Clock, Mode } from "./Clock";

export function TickingClock({
  timestamp,
  mode,
}: {
  timestamp: number,
  mode: Mode,
}) {
  let [duration, setDuration] = useState(0);

  useRequestAnimationFrame(() => {
    setDuration(Date.now() - timestamp);
  }, {});

  return <Clock duration={duration} mode={mode} />;
}
