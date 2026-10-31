'use client';

import { useEffect, useMemo, useState, type CSSProperties, type FormEvent } from 'react';
import { Clock } from 'lucide-react';
import { PROFILE } from '@/lib/zubair-profile';

const CONFIG = {
  hostName: 'Zubair Hussain',
  eventTitle: '30 Minute Meeting',
  description: 'Pick a time that works for you and we’ll talk over Google Meet.',
  duration: 30,
  workDays: [1, 2, 3, 4, 5], // 0=Sun … 6=Sat
  maxDate: '2030-12-31',
  // Google Apps Script Web App URL. Empty = requests go by email instead.
  apiUrl: process.env.NEXT_PUBLIC_BOOKING_API_URL?.trim() ?? '',
};

// Always a white card, whichever site theme is active.
const WHITE_THEME = {
  '--foreground': '222 47% 11%',
  '--muted-foreground': '220 10% 40%',
  '--border': '220 13% 90%',
  '--background': '0 0% 100%',
  background: '#fff',
  color: '#0f172a',
} as CSSProperties;

const ymd = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

async function getSlots(date: string): Promise<string[]> {
  if (CONFIG.apiUrl) {
    const res = await fetch(`${CONFIG.apiUrl}?date=${date}`);
    return ((await res.json()) as { slots?: string[] }).slots ?? [];
  }
  const out: string[] = [];
  for (let h = 9; h < 17; h++) {
    for (const m of ['00', '30']) {
      const dt = new Date(`${date}T${String(h).padStart(2, '0')}:${m}:00`);
      if (dt > new Date()) out.push(dt.toISOString());
    }
  }
  return out;
}

export default function BookingWidget() {
  const today = useMemo(() => {
    const d = new Date();
    d.setHours(0, 0, 0, 0);
    return d;
  }, []);
  const last = useMemo(() => new Date(`${CONFIG.maxDate}T00:00:00`), []);
  const [tz, setTz] = useState('UTC');
  const [place, setPlace] = useState('');
  const [now, setNow] = useState<Date | null>(null);
  const [view, setView] = useState(() => {
    const d = new Date(today);
    while (!CONFIG.workDays.includes(d.getDay())) d.setDate(d.getDate() + 1);
    return new Date(d.getFullYear(), d.getMonth(), 1);
  });
  const [step, setStep] = useState<'pick' | 'form' | 'done'>('pick');
  const [selDate, setSelDate] = useState<string | null>(null);
  const [selSlot, setSelSlot] = useState<string | null>(null);
  const [slots, setSlots] = useState<string[] | null>(null);
  const [slotError, setSlotError] = useState(false);
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [notes, setNotes] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [meetLink, setMeetLink] = useState('');

  useEffect(() => {
    setTz(Intl.DateTimeFormat().resolvedOptions().timeZone);
    // IP location wins; the browser time zone is the fallback.
    fetch('/api/geo')
      .then((res) => res.json())
      .then((data: { timezone?: string; city?: string; country?: string }) => {
        if (!data.timezone) return;
        new Intl.DateTimeFormat([], { timeZone: data.timezone }); // throws if invalid
        setTz(data.timezone);
        setPlace([data.city, data.country].filter(Boolean).join(', '));
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    setNow(new Date());
    const timer = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(timer);
  }, []);

  const fmtT = (iso: string) => new Date(iso).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit', timeZone: tz });
  const fmtD = (iso: string) =>
    new Date(iso).toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric', timeZone: tz });

  const selectDate = async (date: string) => {
    setSelDate(date);
    setSelSlot(null);
    setSlots(null);
    setSlotError(false);
    try {
      setSlots(await getSlots(date));
    } catch {
      setSlotError(true);
    }
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!selSlot) return;
    setError('');
    setBusy(true);
    try {
      if (CONFIG.apiUrl) {
        const res = await fetch(CONFIG.apiUrl, {
          method: 'POST',
          headers: { 'Content-Type': 'text/plain;charset=utf-8' },
          body: JSON.stringify({ start: selSlot, duration: CONFIG.duration, tz, place, name, email, notes }),
        });
        const data = (await res.json()) as { ok?: boolean; error?: string; meetLink?: string };
        if (!data.ok) throw new Error(data.error || 'Failed');
        setMeetLink(data.meetLink?.startsWith('https://') ? data.meetLink : '');
        setStep('done');
      } else {
        const body = `Name: ${name}\nEmail: ${email}\nTime: ${fmtT(selSlot)}, ${fmtD(selSlot)} (${tz})\n\n${notes}`;
        window.location.href = `mailto:${PROFILE.email}?subject=${encodeURIComponent('Meeting request')}&body=${encodeURIComponent(body)}`;
      }
    } catch (err) {
      setError(`Sorry, that time could not be booked. ${err instanceof Error ? err.message : ''}`);
    }
    setBusy(false);
  };

  const first = view.getDay();
  const count = new Date(view.getFullYear(), view.getMonth() + 1, 0).getDate();
  const P = 'hsl(var(--primary))';
  const muted = 'text-sm text-[hsl(var(--muted-foreground))]';
  const input = 'w-full rounded-lg border border-[hsl(var(--border))] bg-transparent p-2.5 text-sm outline-none focus:border-[hsl(var(--primary))]';

  return (
    <div className="flex flex-wrap" style={WHITE_THEME}>
      <aside className="flex-[1_1_240px] border-[hsl(var(--border))] p-7 md:border-r">
        <p className="font-display text-3xl italic">ZH<span style={{ color: P }}>.</span></p>
        <p className={`mt-5 ${muted}`}>{CONFIG.hostName}</p>
        <h2 className="mt-1 text-2xl font-semibold">{CONFIG.eventTitle}</h2>
        <p className={`mt-3 ${muted}`}>{CONFIG.duration} minutes</p>
        <p className={`mt-1 ${muted}`}>Google Meet link sent by email</p>
        {step !== 'pick' && selSlot && <p className="mt-1 text-sm" style={{ color: P }}>{fmtT(selSlot)}, {fmtD(selSlot)}</p>}
        <p className={`mt-4 ${muted}`}>{CONFIG.description}</p>
      </aside>

      {step === 'pick' && (
        <section className="flex flex-[2_1_440px] flex-wrap gap-6 p-7">
          <div className="min-w-[260px] flex-1">
            <h3 className="mb-3 font-semibold">Select a date &amp; time</h3>
            <div className="mb-3 flex items-center justify-between text-sm">
              <button type="button" className="px-2 py-1 disabled:opacity-30" style={{ color: P }} disabled={view <= new Date(today.getFullYear(), today.getMonth(), 1)} onClick={() => setView(new Date(view.getFullYear(), view.getMonth() - 1, 1))}>Prev</button>
              <b>{view.toLocaleDateString([], { month: 'long', year: 'numeric' })}</b>
              <button type="button" className="px-2 py-1 disabled:opacity-30" style={{ color: P }} disabled={view >= new Date(last.getFullYear(), last.getMonth(), 1)} onClick={() => setView(new Date(view.getFullYear(), view.getMonth() + 1, 1))}>Next</button>
            </div>
            <div className="grid grid-cols-7 gap-1.5 text-center">
              {['S', 'M', 'T', 'W', 'T', 'F', 'S'].map((d, i) => <span key={i} className="text-[11px] font-semibold text-[hsl(var(--muted-foreground))]">{d}</span>)}
              {Array.from({ length: first }, (_, i) => <span key={`e${i}`} />)}
              {Array.from({ length: count }, (_, i) => {
                const dt = new Date(view.getFullYear(), view.getMonth(), i + 1);
                const ok = dt >= today && dt <= last && CONFIG.workDays.includes(dt.getDay());
                const key = ymd(dt);
                const sel = selDate === key;
                return (
                  <button
                    key={key}
                    type="button"
                    disabled={!ok}
                    onClick={() => selectDate(key)}
                    className={`aspect-square rounded-full text-sm ${ok ? 'font-bold' : 'text-[hsl(var(--muted-foreground))] opacity-50'}`}
                    style={ok ? { background: sel ? P : 'hsl(var(--primary) / 0.15)', color: sel ? '#fff' : P } : undefined}
                  >{i + 1}</button>
                );
              })}
            </div>
            <p className={`mt-3 flex items-center gap-2 ${muted}`}>
              <Clock size={15} aria-hidden="true" />
              {now && <b>{now.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit', second: '2-digit', timeZone: tz })}</b>}
              <span>{place || tz}</span>
            </p>
            {!selDate && <p className={`mt-2 ${muted}`}>Select a date to see available times.</p>}
          </div>

          {selDate && (
            <div className="max-h-[380px] flex-[0_0_210px] overflow-auto" data-lenis-prevent="true">
              <p className="mb-3 text-sm font-bold">{new Date(`${selDate}T12:00:00Z`).toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric', timeZone: 'UTC' })}</p>
              {slotError && <p className="text-sm text-red-500">Could not load times.</p>}
              {!slotError && !slots && <p className={muted}>Loading times…</p>}
              {slots?.length === 0 && <p className={muted}>No times available on this day.</p>}
              {slots?.map((s) => s === selSlot ? (
                <div key={s} className="mb-2 flex gap-1.5">
                  <span className="flex-1 rounded-lg bg-gray-600 p-3 text-center text-sm font-bold text-white">{fmtT(s)}</span>
                  <button type="button" className="flex-1 rounded-lg p-3 text-sm font-bold text-white" style={{ background: P }} onClick={() => setStep('form')}>Next</button>
                </div>
              ) : (
                <button key={s} type="button" onClick={() => setSelSlot(s)} className="mb-2 block w-full rounded-lg border p-3 text-sm font-bold" style={{ borderColor: P, color: P }}>{fmtT(s)}</button>
              ))}
            </div>
          )}
        </section>
      )}

      {step === 'form' && (
        <form onSubmit={submit} className="flex-[2_1_440px] p-7">
          <button type="button" className="text-sm font-bold" style={{ color: P }} onClick={() => setStep('pick')}>Back</button>
          <h3 className="mt-2 font-semibold">Enter your details</h3>
          <label className="mt-3 block text-sm font-semibold" htmlFor="book-name">Name</label>
          <input id="book-name" className={input} required value={name} onChange={(e) => setName(e.target.value)} />
          <label className="mt-3 block text-sm font-semibold" htmlFor="book-email">Email</label>
          <input id="book-email" type="email" className={input} required value={email} onChange={(e) => setEmail(e.target.value)} />
          <label className="mt-3 block text-sm font-semibold" htmlFor="book-notes">Anything you’d like to share beforehand?</label>
          <textarea id="book-notes" rows={3} className={input} value={notes} onChange={(e) => setNotes(e.target.value)} />
          <button className="btn-primary mt-4 text-xs" disabled={busy}>
            {busy ? 'Scheduling…' : CONFIG.apiUrl ? 'Schedule event' : 'Send request by email'}
          </button>
          {error && <p className="mt-2 text-sm text-red-500">{error}</p>}
        </form>
      )}

      {step === 'done' && selSlot && (
        <section className="flex-[2_1_440px] p-10 text-center">
          <h3 className="text-xl font-semibold">You’re scheduled</h3>
          <p className={`mt-2 ${muted}`}>A calendar invitation was sent to your email.</p>
          <p className="mt-3">{fmtT(selSlot)}, {fmtD(selSlot)}</p>
          {meetLink && <p className="mt-3 break-all">Google Meet: <a href={meetLink} style={{ color: P }}>{meetLink}</a></p>}
        </section>
      )}
    </div>
  );
}
