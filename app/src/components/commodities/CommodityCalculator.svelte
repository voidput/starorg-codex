<script lang="ts">
  /**
   * Buy/sell calculator under a commodity's kiosk prices: an amount in SCU (typed, or filled
   * from one of your ships' cargo grids), a buy and a sell kiosk (cheapest and best by
   * default), and the cost, revenue, profit, profit per SCU and margin. The math lives in
   * `lib/tradecalc.ts`.
   *
   * Lazy: the fleet (`fleet_view`, what the Ships tab lists) loads the first time the
   * calculator gets focus, once per channel; an answer for another channel is dropped.
   */
  import { onDestroy } from 'svelte';
  import { Input } from '@/components/ui/input';
  import SelectField from '@/components/common/SelectField.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { CommodityDetail } from '@/lib/contract/types';
  import { latest, type Load } from '@/lib/loadstate';
  import { priceText } from '@/lib/sources';
  import { app } from '@/lib/stores/app.svelte';
  import {
    fleetShips,
    kioskKey,
    kioskLabel,
    loadCalcPrefs,
    marginText,
    parseScu,
    pickKiosk,
    saveCalcPrefs,
    shipLabel,
    signedAuec,
    tradeCalc,
    type FleetShip,
  } from '@/lib/tradecalc';
  import { cn } from '@/lib/utils';

  interface Props {
    c: CommodityDetail;
    /** "fetched 21 Nov 2026", as the price sections show it. */
    when: string | null;
  }
  let { c, when }: Props = $props();

  const prefs = loadCalcPrefs();
  let amount = $state(prefs.amount);
  let hull = $state<string | null>(prefs.hull);

  // Kiosk picks belong to one commodity; another commodity starts on the defaults.
  let picks = $state<{ id: string; buy: string | null; sell: string | null }>({ id: '', buy: null, sell: null });
  const buyRow = $derived(pickKiosk(c.market.buy, picks.id === c.id ? picks.buy : null));
  const sellRow = $derived(pickKiosk(c.market.sell, picks.id === c.id ? picks.sell : null));
  const pick = (side: 'buy' | 'sell', key: string) => {
    const base = picks.id === c.id ? picks : { id: c.id, buy: null, sell: null };
    picks = { ...base, [side]: key };
  };

  const scu = $derived(parseScu(amount));
  // UEX's files carry no stock to buy or demand to sell (the sell side's `scu_stock` is
  // what the kiosk already holds), so the amount is never capped here.
  const r = $derived(tradeCalc({ scu, buy: buyRow?.price ?? null, sell: sellRow?.price ?? null }));

  // --- Fleet, loaded on first focus ----------------------------------------------------------
  let fleet = $state<Load<FleetShip[], ContractError>>({ state: 'idle' });
  let fleetFor = $state('');
  const loader = latest<FleetShip[], ContractError>((l) => (fleet = l), toContractError);
  onDestroy(() => loader.stop());

  function loadFleet() {
    const key = app.targetKey;
    const target = app.target ? $state.snapshot(app.target) : null;
    if (!key || !target || (fleetFor === key && fleet.state !== 'error')) return;
    fleetFor = key;
    void loader.run(async () => fleetShips(await api.fleetView({ target, read_game_data: false })));
  }

  const ships = $derived(fleet.state === 'ok' && fleetFor === app.targetKey ? fleet.value : []);
  const shipOptions = $derived(ships.map((s) => ({ id: s.hull, label: shipLabel(s) })));
  const shipPlaceholder = $derived(fleet.state === 'loading' ? 'Loading your ships…' : fleet.state === 'ok' && !ships.length ? 'No ships in your fleet' : 'Fill from a ship');

  function remember() {
    saveCalcPrefs({ amount, hull });
  }
  function pickShip(h: string) {
    hull = h;
    const s = ships.find((x) => x.hull === h);
    // A ship whose cargo is not known leaves the amount alone.
    if (s?.scu != null) amount = String(s.scu);
    remember();
  }
  function typeAmount(text: string) {
    amount = text;
    const s = ships.find((x) => x.hull === hull);
    if (s && s.scu !== parseScu(text)) hull = null;
    remember();
  }

  const buyOptions = $derived(c.market.buy.map((k) => ({ id: kioskKey(k), label: kioskLabel(k) })));
  const sellOptions = $derived(c.market.sell.map((k) => ({ id: kioskKey(k), label: kioskLabel(k) })));
  const scuText = (v: number) => `${v.toLocaleString('en-US', { maximumFractionDigits: 3 })} SCU`;
</script>

<section class="grid gap-2" aria-labelledby="c-calc" onfocusin={loadFleet}>
  <h3 id="c-calc" class="eyebrow text-muted-foreground">
    Trade calculator <span class="font-normal normal-case">· UEX, player-reported{when ? `, ${when}` : ''}</span>
  </h3>
  <div class="grid gap-2 text-[12.5px] sm:grid-cols-[8rem_minmax(0,1fr)]">
    <label class="text-muted-foreground self-center text-[12px]" for="calc-amount">Amount (SCU)</label>
    <div class="flex flex-wrap items-center gap-2">
      <Input
        id="calc-amount"
        type="text"
        inputmode="decimal"
        autocomplete="off"
        class="h-8 w-28 tabular-nums"
        placeholder="e.g. 96"
        aria-invalid={amount.trim() !== '' && scu === null}
        value={amount}
        oninput={(e) => typeAmount((e.currentTarget as HTMLInputElement).value)}
      />
      <SelectField
        options={shipOptions}
        value={ships.some((s) => s.hull === hull) ? hull : null}
        onchange={pickShip}
        ariaLabel="Fill the amount from one of your ships' cargo"
        placeholder={shipPlaceholder}
        disabled={fleet.state === 'loading'}
        class="max-w-64 min-w-40"
      />
      {#if fleet.state === 'error'}<span class="text-muted-foreground text-[11.5px]">Could not read your fleet: {fleet.error.message}</span>{/if}
    </div>

    <span class="text-muted-foreground self-center text-[12px]">Buy at</span>
    {#if buyRow}
      <SelectField options={buyOptions} value={kioskKey(buyRow)} onchange={(k) => pick('buy', k)} ariaLabel="Kiosk to buy {c.name} at" class="w-full max-w-md" />
    {:else}
      <span class="text-muted-foreground text-[12px]">No kiosk sells it in the UEX data.</span>
    {/if}

    <span class="text-muted-foreground self-center text-[12px]">Sell at</span>
    {#if sellRow}
      <SelectField options={sellOptions} value={kioskKey(sellRow)} onchange={(k) => pick('sell', k)} ariaLabel="Kiosk to sell {c.name} at" class="w-full max-w-md" />
    {:else}
      <span class="text-muted-foreground text-[12px]">No kiosk buys it in the UEX data.</span>
    {/if}
  </div>

  {#if r.scu > 0}
    <dl class="grid grid-cols-[repeat(auto-fit,minmax(7.5rem,1fr))] gap-2 text-[12.5px]" aria-live="polite">
      <div class="grid gap-0.5">
        <dt class="text-muted-foreground text-[11px]">Cost</dt>
        <dd class="m-0 font-medium tabular-nums">{r.cost !== null ? priceText(r.cost) : '–'}</dd>
      </div>
      <div class="grid gap-0.5">
        <dt class="text-muted-foreground text-[11px]">Revenue</dt>
        <dd class="m-0 font-medium tabular-nums">{r.revenue !== null ? priceText(r.revenue) : '–'}</dd>
      </div>
      <div class="grid gap-0.5">
        <dt class="text-muted-foreground text-[11px]">Profit</dt>
        <dd class={cn('m-0 font-semibold tabular-nums', r.profit !== null && r.profit < 0 && 'text-destructive')}>{signedAuec(r.profit)}</dd>
      </div>
      <div class="grid gap-0.5">
        <dt class="text-muted-foreground text-[11px]">Profit per SCU</dt>
        <dd class="m-0 tabular-nums">{r.perScu !== null ? `${signedAuec(r.perScu, 2)}/SCU` : '–'}</dd>
      </div>
      <div class="grid gap-0.5">
        <dt class="text-muted-foreground text-[11px]" title="Profit as a share of the cost">Margin</dt>
        <dd class="m-0 tabular-nums">{marginText(r.margin)}</dd>
      </div>
    </dl>
    <p class="text-muted-foreground m-0 text-[11px]">
      For {scuText(r.scu)}. {#if !buyRow || !sellRow}Profit needs both a buy and a sell kiosk.{/if}
      Not capped: the UEX data has no stock to buy or demand to sell. Prices change; check the kiosk.
    </p>
  {:else}
    <p class="text-muted-foreground m-0 text-[12px]">{amount.trim() && scu === null ? 'Type an amount above 0 SCU, like 96 or 0.5.' : 'Type an amount or pick a ship to see cost and profit.'}</p>
  {/if}
</section>
