import { useEffect, useState } from "react";
import { Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle } from "@fluentui/react-components";
import { StatTile } from "@/kit";
import { distance, timeText, useWeek } from "@/cards/week/week";
import { goTo } from "@/store";
import { weekMomentAlways } from "@/discover";

const SHOWN_KEY = "hrtz.weekShown";
const DAY = 86_400_000;

/**
 * Your week, once a week: a pop-up with the week's headline figures when there's a week worth
 * showing (three days of use or more) and it hasn't been shown for seven days. (The features lab's
 * "moments" direction.)
 */
export function WeekMoment() {
  const week = useWeek();
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!week || open) return;
    const used = week.days.filter((d) => d.cm > 0).length;
    const last = Number(localStorage.getItem(SHOWN_KEY) ?? 0);
    if (weekMomentAlways() || (used >= 3 && Date.now() - last >= 7 * DAY)) {
      localStorage.setItem(SHOWN_KEY, String(Date.now()));
      setOpen(true);
    }
  }, [week]);
  if (!week) return null;
  return (
    <Dialog open={open} onOpenChange={(_, d) => setOpen(d.open)}>
      <DialogSurface style={{ maxWidth: 560 }}>
        <DialogBody>
          <DialogTitle>Your week with the mouse</DialogTitle>
          <DialogContent>
            <div className="tw grid grid-cols-3 gap-2 pt-2">
              <StatTile label="Distance" value={distance(week.cm)} detail="This week" />
              <StatTile label="Time moving" value={timeText(week.movingS)} detail={`${timeText(week.movingS / 7)} a day`} />
              <StatTile label="Fastest" value={week.topMps > 0 ? `${week.topMps.toFixed(1)} m/s` : "–"} detail="Your quickest flick" />
            </div>
          </DialogContent>
          <DialogActions>
            <Button onClick={() => setOpen(false)}>Not now</Button>
            <Button
              appearance="primary"
              onClick={() => {
                setOpen(false);
                goTo("week");
              }}
            >
              See your week
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
