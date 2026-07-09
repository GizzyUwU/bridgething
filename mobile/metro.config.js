const path = require('path');
const { getDefaultConfig } = require('@react-native/metro-config');
const { withNativeWind } = require('nativewind/metro');
const {
  wrapWithReanimatedMetroConfig,
} = require('react-native-reanimated/metro-config');

const projectRoot = __dirname;
const workspaceRoot = path.resolve(projectRoot, '..');

const config = getDefaultConfig(projectRoot);

config.watchFolders = [workspaceRoot];

// @rn-primitives/portal keeps its portal registry in a module-level zustand store.
// its package exports split import->esm and require->cjs, so App.tsx (import) and the
// compiled primitives (require) would each load a different build with its own store:
// dialogs register into one, <PortalHost/> reads the other, and nothing ever mounts.
// pin every importer to the single cjs build so they share one registry.
const portalEntry = require.resolve('@rn-primitives/portal');
const baseResolveRequest = config.resolver.resolveRequest;
config.resolver.resolveRequest = (context, moduleName, platform) => {
  if (moduleName === '@rn-primitives/portal') {
    return { type: 'sourceFile', filePath: portalEntry };
  }
  return baseResolveRequest
    ? baseResolveRequest(context, moduleName, platform)
    : context.resolveRequest(context, moduleName, platform);
};

module.exports = wrapWithReanimatedMetroConfig(
  withNativeWind(config, { input: './global.css', inlineRem: 16 }),
);
