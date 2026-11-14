import { Bell, CalendarX } from "lucide-react";
import { useState } from "react";
import {
  Badge, Button, Card, Dialog, EmptyState, IconButton, ListGroup, ListRow, Segmented, Select, Sheet, Skeleton,
  Switch, TextField, Tooltip, calendarColour, CALENDAR_COLOURS, useTheme, useToasts, type ThemeChoice,
} from "./index";

function Block({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-10">
      <h2 className="mb-3 text-headline font-semibold text-fg">{title}</h2>
      <div className="flex flex-wrap items-center gap-3">{children}</div>
    </section>
  );
}

/** Development-only review page for every primitive: open `/#/gallery`. */
export function Gallery() {
  const theme = useTheme();
  const push = useToasts((s) => s.push);
  const [on, setOn] = useState(true);
  const [sel, setSel] = useState("10");
  const [seg, setSeg] = useState("day");
  const [dialog, setDialog] = useState(false);
  const [sheet, setSheet] = useState(false);

  return (
    <div className="mx-auto h-full max-w-3xl overflow-y-auto px-8 py-10">
      <div className="mb-8 flex items-center justify-between">
        <h1 className="text-largetitle font-bold tracking-tight">Design gallery</h1>
        <Segmented
          label="Theme"
          value={theme.choice}
          onValueChange={(v) => theme.set(v as ThemeChoice)}
          options={[{ value: "system", label: "Auto" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]}
        />
      </div>

      <Block title="Type scale">
        <div className="flex flex-col gap-1">
          <p className="text-display font-bold tracking-tight">Display 34</p>
          <p className="text-largetitle font-bold tracking-tight">Large title 28</p>
          <p className="text-title font-semibold">Title 20</p>
          <p className="text-headline font-semibold">Headline 15</p>
          <p className="text-body">Body 13 — The quick brown fox 10:30 – 11:30</p>
          <p className="text-footnote text-fg-2">Footnote 12 secondary</p>
          <p className="text-caption text-fg-3">Caption 11 tertiary</p>
        </div>
      </Block>

      <Block title="Colour">
        {(["bg", "surface", "surface-raised", "fill", "accent", "danger", "warning", "success"] as const).map((n) => (
          <div key={n} className="flex flex-col items-center gap-1">
            <div className="size-12 rounded-card shadow-card" style={{ background: `var(--${n})` }} />
            <span className="text-caption text-fg-2">{n}</span>
          </div>
        ))}
      </Block>

      <Block title="Calendar palette">
        {CALENDAR_COLOURS.map((c) => (
          <div key={c} className="flex flex-col items-center gap-1">
            <div className="h-6 w-1 rounded-full" style={{ background: calendarColour(c) }} />
            <span className="text-caption text-fg-2">{c}</span>
          </div>
        ))}
      </Block>

      <Block title="Buttons">
        <Button variant="primary">Primary</Button>
        <Button variant="secondary">Secondary</Button>
        <Button variant="plain">Plain</Button>
        <Button variant="danger">Danger</Button>
        <Button variant="secondary" size="sm">Small</Button>
        <Button disabled>Disabled</Button>
        <Tooltip content="Notifications"><IconButton label="Notifications"><Bell className="size-[18px]" /></IconButton></Tooltip>
      </Block>

      <Block title="Controls">
        <Switch label="Demo switch" checked={on} onCheckedChange={setOn} />
        <Select label="Minutes" value={sel} onValueChange={setSel} options={[{ value: "5", label: "5 minutes" }, { value: "10", label: "10 minutes" }, { value: "15", label: "15 minutes" }]} />
        <Segmented label="View" value={seg} onValueChange={setSeg} options={[{ value: "day", label: "Day" }, { value: "week", label: "Week" }]} />
        <div className="w-64"><TextField label="Account name" placeholder="Work" hint="Shown in the sidebar." /></div>
        <div className="w-64"><TextField label="Server" defaultValue="caldav" error="Enter a full address." /></div>
      </Block>

      <Block title="Surfaces">
        <Card className="w-64"><p className="text-headline font-semibold">Card</p><p className="text-body text-fg-2">Soft shadow, no border.</p></Card>
        <ListGroup className="w-64">
          <ListRow title="First row" subtitle="Secondary line" trailing="9:00" />
          <ListRow title="Tappable row" subtitle="Hover me" onClick={() => push({ title: "Row tapped" })} />
        </ListGroup>
        <Badge>Neutral</Badge><Badge tone="accent">Accent</Badge><Badge tone="warning">Warning</Badge><Badge tone="danger">Danger</Badge>
      </Block>

      <Block title="States">
        <div className="w-72 rounded-card bg-surface shadow-card"><EmptyState icon={<CalendarX />} title="Your day is clear" description="No events today or tomorrow." action={<Button>Add account</Button>} /></div>
        <div className="flex w-64 flex-col gap-2"><Skeleton className="h-5 w-24" /><Skeleton className="h-16" /></div>
      </Block>

      <Block title="Overlays">
        <Button onClick={() => setDialog(true)}>Open dialog</Button>
        <Button onClick={() => setSheet(true)}>Open sheet</Button>
        <Button onClick={() => push({ title: "Saved", description: "Your changes are stored." })}>Toast</Button>
        <Button variant="danger" onClick={() => push({ title: "Couldn't sync", tone: "danger" })}>Error toast</Button>
      </Block>

      <Dialog open={dialog} onOpenChange={setDialog} title="Remove account?" description="Cached events for this account will be deleted." footer={<><Button onClick={() => setDialog(false)}>Cancel</Button><Button variant="danger" onClick={() => setDialog(false)}>Remove</Button></>}>
        <p className="text-body text-fg-2">You can add it again at any time.</p>
      </Dialog>
      <Sheet open={sheet} onOpenChange={setSheet} title="Sheet"><p className="pb-6 text-body text-fg-2">Slides up from the bottom.</p></Sheet>
    </div>
  );
}
