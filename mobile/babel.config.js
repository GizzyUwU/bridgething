module.exports = {
  presets: ['module:@react-native/babel-preset', 'nativewind/babel'],
  // react-native-worklets/plugin must be the last plugin (Reanimated 4 + worklets requirement).
  plugins: ['react-native-worklets/plugin'],
};
