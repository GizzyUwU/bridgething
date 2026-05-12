module.exports = {
  presets: [
    require.resolve('@react-native/babel-preset'),
    require.resolve('nativewind/babel'),
  ],
  // react-native-worklets/plugin must be the last plugin
  plugins: [require.resolve('react-native-worklets/plugin')],
};
