import InAppBrowser from 'react-native-inappbrowser-reborn';
import { Linking } from 'react-native';

import { PALETTE } from './theme';

/**
 * Open the device-code verification URL in an in-app browser. The
 * native side fired `authStateChanged({ kind: 'pending', ... })` with
 * the pre-filled URL; this is the JS-owned counterpart that used to
 * live in HybridBridgethingSessionImpl as SFSafariViewController glue.
 *
 * Falls back to Linking.openURL on simulators / Macs where the
 * SFSafariViewController-backed module isn't available.
 */
export async function openVerificationBrowser(url: string): Promise<void> {
  try {
    if (await InAppBrowser.isAvailable()) {
      await InAppBrowser.open(url, {
        // iOS — keep matching the rest of the app's chrome.
        dismissButtonStyle: 'cancel',
        preferredBarTintColor: PALETTE.light.background,
        preferredControlTintColor: PALETTE.light.primary,
        modalPresentationStyle: 'formSheet',
        modalEnabled: true,
        animated: true,
        // Android.
        showTitle: true,
        toolbarColor: PALETTE.light.background,
        secondaryToolbarColor: PALETTE.light.surface,
        navigationBarColor: PALETTE.light.background,
        enableUrlBarHiding: true,
        enableDefaultShare: false,
        forceCloseOnRedirection: false,
        showInRecents: false,
      });
      return;
    }
  } catch (err) {
    console.warn('[bridgething] InAppBrowser open failed, falling back', err);
  }
  await Linking.openURL(url);
}

/** Dismiss any open in-app browser. No-op if nothing is presented or
 *  the host doesn't support programmatic dismiss. */
export async function dismissVerificationBrowser(): Promise<void> {
  try {
    if (await InAppBrowser.isAvailable()) {
      InAppBrowser.close();
    }
  } catch {
    // Nothing meaningful to do — this fires from event handlers.
  }
}
