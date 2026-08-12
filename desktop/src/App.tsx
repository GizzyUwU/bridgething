import type { VNode } from 'preact';
import { LocationProvider, Route, Router, useLocation } from 'preact-iso';
import { useEffect, useRef } from 'preact/hooks';

import { Sidebar } from './components/Sidebar.tsx';
import { useDesktop } from './desktop.ts';
import { PATHS } from './routes.ts';
import { AppDetailRoute, AppsRoute } from './screens/Apps.tsx';
import { DevicesScreen } from './screens/Devices.tsx';
import { LogsScreen } from './screens/Logs.tsx';
import { OnboardingScreen } from './screens/Onboarding.tsx';
import { SettingsScreen } from './screens/Settings.tsx';
import { CatalogAppRoute, SourceAppRoute, SourceRoute, StoreRoute } from './screens/Store.tsx';
import { UpdatesScreen } from './screens/Updates.tsx';
import { completeFirstRun, firstRunDone } from './stores/first-run.ts';
import { keptRoute, peers } from './stores/session.ts';

export function App(): VNode {
  return (
    <LocationProvider>
      <Shell />
    </LocationProvider>
  );
}

function Shell(): VNode {
  const session = useDesktop();
  const { url, route } = useLocation();
  const linked = peers.value;
  const connected = linked.some(peer => peer.status === 'connected');
  const onboarding = !firstRunDone.value && !connected;

  const restored = useRef(false);

  useEffect(() => {
    if (connected) completeFirstRun();
  }, [connected]);

  useEffect(() => {
    if (!restored.current) {
      restored.current = true;
      const held = keptRoute.data.value;
      if (held !== null && held !== url) route(held, true);
      return;
    }
    void session.setRoute(url);
  }, [session, route, url]);

  if (onboarding) return <OnboardingScreen onSkip={completeFirstRun} />;

  return (
    <div class="flex h-full min-w-0">
      <Sidebar peers={linked} />
      <div key={url} class="route-enter flex min-h-0 min-w-0 flex-1">
        <Router>
          <Route path={PATHS.devices} component={DevicesScreen} />
          <Route path={PATHS.apps} component={AppsRoute} />
          <Route path="/apps/:webappId" component={AppDetailRoute} />
          <Route path={PATHS.store} component={StoreRoute} />
          <Route path="/store/app/:appId" component={CatalogAppRoute} />
          <Route path="/store/source/:source" component={SourceRoute} />
          <Route path="/store/source/:source/app/:appId" component={SourceAppRoute} />
          <Route path={PATHS.updates} component={UpdatesScreen} />
          <Route path={PATHS.logs} component={LogsScreen} />
          <Route path={PATHS.settings} component={SettingsScreen} />
          <Route default component={DevicesScreen} />
        </Router>
      </div>
    </div>
  );
}
