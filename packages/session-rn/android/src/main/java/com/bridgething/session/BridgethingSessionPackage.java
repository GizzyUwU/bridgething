package com.bridgething.session;

import androidx.annotation.Nullable;

import com.facebook.react.BaseReactPackage;
import com.facebook.react.bridge.NativeModule;
import com.facebook.react.bridge.ReactApplicationContext;
import com.facebook.react.module.model.ReactModuleInfoProvider;
import com.margelo.nitro.bridgething.session.BridgethingSessionOnLoad;

import java.util.HashMap;

/**
 * Empty React package that exists so RN's autolinker has something to
 * import on the android side. The real nitro hybrid object is
 * registered via {@link BridgethingSessionOnLoad#initializeNative()}
 * which runs from the static initializer below when this class is loaded.
 * Mirrors the shape react-native-mmkv uses for the same reason.
 */
public class BridgethingSessionPackage extends BaseReactPackage {
  @Nullable
  @Override
  public NativeModule getModule(String name, ReactApplicationContext reactContext) {
    return null;
  }

  @Override
  public ReactModuleInfoProvider getReactModuleInfoProvider() {
    return HashMap::new;
  }

  static {
    BridgethingSessionOnLoad.initializeNative();
  }
}
