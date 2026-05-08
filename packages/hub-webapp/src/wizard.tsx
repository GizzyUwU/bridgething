import {
  type BluetoothPairingResult,
  type BluetoothPin,
  type BridgethingClient,
  type ConnectedDevice,
} from '@bridgething/client';
import { useEffect, useState } from 'react';

const PHASE_KEY = 'onboarding_phase';

type Step = 'pair' | 'gesture';

export function Wizard({ client, onDone }: { client: BridgethingClient; onDone: () => void }) {
  const [step, setStep] = useState<Step>('pair');

  const finish = async () => {
    await client.store.put({ key: PHASE_KEY, value: 'done' });
    onDone();
  };

  return (
    <div className="flex h-full w-full flex-col bg-bt-charcoal">
      <StepIndicator step={step} />
      <div className="flex-1 overflow-hidden">
        {step === 'pair' && <PairStep client={client} onNext={() => setStep('gesture')} />}
        {step === 'gesture' && <GestureStep onNext={finish} />}
      </div>
    </div>
  );
}

function StepIndicator({ step }: { step: Step }) {
  const order: Step[] = ['pair', 'gesture'];
  const idx = order.indexOf(step);
  return (
    <div className="flex items-center justify-center gap-2 pt-4">
      {order.map((s, i) => (
        <div
          key={s}
          className={`h-1.5 w-8 rounded-full transition ${i <= idx ? 'bg-bt-blue' : 'bg-bt-soft-gray/30'}`}
        />
      ))}
    </div>
  );
}

function PairStep({ client, onNext }: { client: BridgethingClient; onNext: () => void }) {
  const [alias, setAlias] = useState<string | null>(null);
  const [pin, setPin] = useState<BluetoothPin | null>(null);
  const [connected, setConnected] = useState<ConnectedDevice | null>(null);
  const [pairResult, setPairResult] = useState<BluetoothPairingResult | null>(null);

  useEffect(() => {
    let cancelled = false;
    client.system.versionRequest().then(r => {
      if (cancelled || !r.ok) return;
      const tail = r.response.serialNumber.slice(-4);
      setAlias(tail ? `Car Thing (SN: ${tail})` : 'Car Thing');
    });
    return () => {
      cancelled = true;
    };
  }, [client]);

  useEffect(() => {
    const offPin = client.bluetooth.onPin(setPin);
    const offConnected = client.bluetooth.onConnectedDevice(setConnected);
    const offResult = client.bluetooth.onPairingResult(setPairResult);
    return () => {
      offPin();
      offConnected();
      offResult();
    };
  }, [client]);

  const ready = pairResult?.success === true || connected !== null;

  return (
    <div className="flex h-full flex-col items-center justify-between px-8 pt-6 pb-8">
      <div className="flex flex-1 flex-col items-center justify-center gap-6">
        <div className="bt-wordmark text-2xl font-medium text-bt-off-white">pair your phone</div>
        <div className="max-w-[26rem] text-center text-sm text-bt-soft-gray">
          {ready ? 'paired. you can move on.' : 'open bluetooth on your phone and look for this device.'}
        </div>

        {!ready && alias && (
          <div className="rounded-2xl border border-bt-soft-gray/30 bg-black/30 px-6 py-4">
            <div className="bt-wordmark text-xl font-medium text-bt-off-white">{alias}</div>
          </div>
        )}

        {pin && !ready && (
          <div className="flex flex-col items-center gap-1">
            <div className="text-xs uppercase tracking-widest text-bt-soft-gray">pin</div>
            <div className="bt-wordmark text-4xl font-medium tracking-[0.4em] text-bt-blue">{pin.pin}</div>
          </div>
        )}

        {ready && connected && (
          <div className="flex flex-col items-center gap-1">
            <div className="text-xs uppercase tracking-widest text-bt-soft-gray">connected</div>
            <div className="text-base text-bt-off-white">{connected.name}</div>
          </div>
        )}
      </div>

      <div className="flex w-full max-w-[26rem] items-center justify-between">
        <button
          type="button"
          onClick={onNext}
          className="text-sm text-bt-soft-gray underline-offset-2 active:underline">
          skip for now
        </button>
        <button
          type="button"
          onClick={onNext}
          disabled={!ready}
          className="rounded-full bg-bt-blue px-8 py-2.5 text-sm font-medium text-bt-charcoal transition active:scale-95 disabled:bg-bt-soft-gray/30 disabled:text-bt-soft-gray">
          next
        </button>
      </div>
    </div>
  );
}

function GestureStep({ onNext }: { onNext: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-between px-8 pt-6 pb-8">
      <div className="flex flex-1 flex-col items-center justify-center gap-8">
        <div className="bt-wordmark text-2xl font-medium text-bt-off-white">jump back to apps</div>
        <div className="max-w-[26rem] text-center text-sm text-bt-soft-gray">
          press the far-right preset button five times to return here from any app.
        </div>

        <PresetRow />
      </div>

      <div className="flex w-full max-w-[26rem] items-center justify-end">
        <button
          type="button"
          onClick={onNext}
          className="rounded-full bg-bt-blue px-8 py-2.5 text-sm font-medium text-bt-charcoal transition active:scale-95">
          done
        </button>
      </div>
    </div>
  );
}

function PresetRow() {
  return (
    <div className="flex items-center gap-3">
      {[0, 1, 2, 3].map(i => {
        const active = i === 3;
        return (
          <div
            key={i}
            className={`flex h-12 w-12 items-center justify-center rounded-full border ${
              active
                ? 'border-bt-blue bg-bt-blue/10 text-bt-blue animate-pulse'
                : 'border-bt-soft-gray/40 text-bt-soft-gray'
            }`}>
            <span className="bt-wordmark text-base font-medium">{i + 1}</span>
          </div>
        );
      })}
    </div>
  );
}
