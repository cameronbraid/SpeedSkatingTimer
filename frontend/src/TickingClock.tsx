import useRequestAnimationFrame from "use-request-animation-frame";
import { useState } from "react";
import { Clock } from "./Clock";

export function TickingClock({
  timestamp,
  warmUpLapTime,
}: {
  timestamp: number;
  warmUpLapTime: boolean;
}) {
  let [duration, setDuration] = useState();

  useRequestAnimationFrame(() => {
    setDuration(Date.now() - timestamp);
  }, {});

  return <Clock duration={duration} warmUpLapTime={warmUpLapTime} />;
}
