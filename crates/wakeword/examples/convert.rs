use std::collections::HashMap;

use bridgething_wakeword::blob::{KIND_CLASSIFIER, KIND_FEATURES, MAGIC, PARAMS, VERSION, tag};
use tract_onnx::{
  data_resolver::FopenDataResolver,
  pb::{GraphProto, NodeProto, TensorProto},
  prelude::*,
  tract_core::internal::{bail, format_err},
};

struct Writer {
  bytes: Vec<u8>,
  ops: usize,
}

impl Writer {
  fn new(kind: u32, params: [usize; PARAMS]) -> Self {
    let mut writer = Self {
      bytes: Vec::new(),
      ops: 0,
    };
    writer.bytes.extend_from_slice(&MAGIC);
    writer.u32(VERSION);
    writer.u32(kind);
    for param in params {
      writer.usize(param);
    }
    writer.u32(0);
    writer
  }

  fn u32(&mut self, value: u32) {
    self.bytes.extend_from_slice(&value.to_le_bytes());
  }

  fn usize(&mut self, value: usize) {
    self.u32(u32::try_from(value).expect("dimensions fit in a u32"));
  }

  fn f32(&mut self, value: f32) {
    self.u32(value.to_bits());
  }

  fn floats(&mut self, values: &[f32]) {
    for value in values {
      self.f32(*value);
    }
  }

  fn op(&mut self, tag: u32) {
    self.ops += 1;
    self.u32(tag);
  }

  fn finish(mut self) -> Vec<u8> {
    let at = 4 + 4 + 4 + PARAMS * 4;
    self.bytes[at..at + 4].copy_from_slice(&u32::try_from(self.ops).expect("op count").to_le_bytes());
    self.bytes
  }
}

struct Weights<'a> {
  tensors: HashMap<&'a str, &'a TensorProto>,
  source: &'a str,
}

fn ints(node: &NodeProto, name: &str) -> Option<Vec<i64>> {
  node
    .attribute
    .iter()
    .find(|attribute| attribute.name == name)
    .map(|attribute| attribute.ints.clone())
}

fn float(node: &NodeProto, name: &str) -> Option<f32> {
  node
    .attribute
    .iter()
    .find(|attribute| attribute.name == name)
    .map(|attribute| attribute.f)
}

fn integer(node: &NodeProto, name: &str) -> Option<i64> {
  node
    .attribute
    .iter()
    .find(|attribute| attribute.name == name)
    .map(|attribute| attribute.i)
}

fn explicit_padding(node: &NodeProto) -> TractResult<()> {
  match node.attribute.iter().find(|attribute| attribute.name == "auto_pad") {
    Some(attribute) if attribute.s.as_slice() != b"NOTSET" => bail!(
      "{} pads by {}, and the converter only reads explicit pads",
      node.name,
      String::from_utf8_lossy(&attribute.s)
    ),
    _ => Ok(()),
  }
}

fn shape(graph: &GraphProto, name: &str) -> TractResult<Vec<usize>> {
  graph
    .input
    .iter()
    .chain(&graph.output)
    .find(|value| value.name == name)
    .and_then(|value| value.r#type.as_ref())
    .and_then(|kind| kind.value.as_ref())
    .map(|value| {
      let tract_onnx::pb::type_proto::Value::TensorType(tensor) = value;
      tensor
        .shape
        .iter()
        .flat_map(|shape| &shape.dim)
        .map(|dim| match dim.value {
          Some(tract_onnx::pb::tensor_shape_proto::dimension::Value::DimValue(size)) => size.max(0) as usize,
          _ => 0,
        })
        .collect()
    })
    .ok_or_else(|| format_err!("{name} has no declared shape"))
}

fn tensor(weights: &Weights, name: &str) -> TractResult<Tensor> {
  let proto = weights
    .tensors
    .get(name)
    .ok_or_else(|| format_err!("{name} is not a constant in this graph"))?;
  tract_onnx::tensor::load_tensor(&FopenDataResolver, proto, Some(weights.source))
}

fn integers(weights: &Weights, name: &str) -> TractResult<Vec<i64>> {
  Ok(tensor(weights, name)?.view().as_slice::<i64>()?.to_vec())
}

fn floats(weights: &Weights, name: &str) -> TractResult<(Vec<usize>, Vec<f32>)> {
  let tensor = tensor(weights, name)?;
  let shape = tensor.shape().to_vec();
  let values = tensor.view().as_slice::<f32>()?.to_vec();
  Ok((shape, values))
}

fn features(graph: &GraphProto, weights: &Weights) -> TractResult<Vec<u8>> {
  let input = shape(graph, &graph.input[0].name)?;
  let output = shape(graph, &graph.output[0].name)?;
  let (frames_per_call, mel_bins) = (input[1], input[2]);
  let (embeddings_per_call, embedding_dim) = (output[1], output[3]);

  let first = &graph.node[0];
  let last = &graph.node[graph.node.len() - 1];
  if first.op_type != "Transpose" || ints(first, "perm") != Some(vec![0, 3, 1, 2]) {
    bail!("expected the graph to open by transposing into NCHW");
  }
  if last.op_type != "Transpose" || ints(last, "perm") != Some(vec![0, 2, 3, 1]) {
    bail!("expected the graph to close by transposing out of NCHW");
  }

  let mut writer = Writer::new(
    KIND_FEATURES,
    [frames_per_call, mel_bins, embedding_dim, embeddings_per_call],
  );
  let mut skip = None;
  let mut current = first.output[0].clone();

  for (index, node) in graph.node.iter().enumerate().skip(1).take(graph.node.len() - 2) {
    if skip == Some(index) {
      continue;
    }
    if !node.input.contains(&current) {
      bail!("{} does not consume the op before it, so the graph branches", node.name);
    }
    match node.op_type.as_str() {
      "Concat" => {
        if !node.input[0].starts_with("cache_in") || integer(node, "axis") != Some(2) {
          bail!("{} concatenates something other than a time cache", node.name);
        }
        let cache = shape(graph, &node.input[0])?;
        writer.op(tag::CACHE);
        writer.usize(cache[2]);
        writer.usize(cache[3]);
        writer.usize(cache[1]);
      }
      "Slice" => {
        if !node.output[0].starts_with("cache_out") {
          bail!("{} slices something other than a cache", node.name);
        }
        let kept = integers(weights, &node.input[1])?[0];
        let axes = integers(weights, &node.input[3])?[0];
        let cache = shape(graph, &node.output[0].replace("cache_out", "cache_in"))?;
        if axes != 2 || kept != -(cache[2] as i64) {
          bail!("{} keeps {kept} on axis {axes}, not the whole time cache", node.name);
        }
      }
      "Conv" => {
        let (shape, values) = floats(weights, &node.input[1])?;
        let [out_channels, in_channels, kernel_frames, kernel_bins] = shape[..] else {
          bail!("{} has a {:?} weight, not a 4d one", node.name, shape)
        };
        explicit_padding(node)?;
        if ints(node, "strides").unwrap_or(vec![1, 1]) != vec![1, 1] {
          bail!("{} strides its convolution, which the runtime does not do", node.name);
        }
        if ints(node, "dilations").unwrap_or(vec![1, 1]) != vec![1, 1] || integer(node, "group").unwrap_or(1) != 1 {
          bail!("{} is dilated or grouped, which the runtime does not do", node.name);
        }
        let pads = ints(node, "pads").unwrap_or(vec![0, 0, 0, 0]);
        if pads[0] != 0 || pads[2] != 0 {
          bail!("{} pads in time, which would break the streaming stride", node.name);
        }
        if pads[1] != pads[3] {
          bail!("{} pads unevenly in frequency", node.name);
        }

        let bias = match node.input.get(2) {
          Some(name) => Some(floats(weights, name)?.1),
          None => None,
        };
        writer.op(tag::CONV);
        writer.usize(out_channels);
        writer.usize(in_channels);
        writer.usize(kernel_frames);
        writer.usize(kernel_bins);
        writer.usize(pads[1] as usize);
        writer.u32(u32::from(bias.is_some()));
        for frame in 0..kernel_frames {
          for bin in 0..kernel_bins {
            for input in 0..in_channels {
              for output in 0..out_channels {
                let at = ((output * in_channels + input) * kernel_frames + frame) * kernel_bins + bin;
                writer.f32(values[at]);
              }
            }
          }
        }
        if let Some(bias) = bias {
          writer.floats(&bias);
        }
      }
      "LeakyRelu" => {
        let slope = float(node, "alpha").ok_or_else(|| format_err!("{} has no slope", node.name))?;
        let following = &graph.node[index + 1];
        let floor = if following.op_type == "Max" && following.input.contains(&node.output[0]) {
          skip = Some(index + 1);
          let other = following
            .input
            .iter()
            .find(|input| *input != &node.output[0])
            .ok_or_else(|| format_err!("{} maxes an output against itself", following.name))?;
          let (_, clamp) = floats(weights, other)?;
          if clamp.len() != 1 {
            bail!("{} clamps against a tensor rather than a scalar", following.name);
          }
          clamp[0]
        } else {
          f32::NEG_INFINITY
        };
        writer.op(tag::ACTIVATION);
        writer.f32(slope);
        writer.f32(floor);
      }
      "MaxPool" => {
        let kernel = ints(node, "kernel_shape").ok_or_else(|| format_err!("{} has no kernel", node.name))?;
        let strides = ints(node, "strides").unwrap_or(vec![1, 1]);
        explicit_padding(node)?;
        if integer(node, "ceil_mode").unwrap_or(0) != 0 {
          bail!(
            "{} rounds its output shape up, which the runtime does not do",
            node.name
          );
        }
        if ints(node, "pads")
          .unwrap_or(vec![0, 0, 0, 0])
          .iter()
          .any(|pad| *pad != 0)
        {
          bail!("{} pads its pooling, which the runtime does not do", node.name);
        }
        writer.op(tag::MAX_POOL);
        writer.usize(kernel[0] as usize);
        writer.usize(kernel[1] as usize);
        writer.usize(strides[0] as usize);
        writer.usize(strides[1] as usize);
      }
      other => bail!("{} is a {other}, which the runtime has no op for", node.name),
    }
    current = match node.op_type.as_str() {
      "Slice" => current,
      _ if skip == Some(index + 1) => graph.node[index + 1].output[0].clone(),
      _ => node.output[0].clone(),
    };
  }
  if !last.input.contains(&current) {
    bail!("the closing transpose does not consume the last op, so the graph branches");
  }

  Ok(writer.finish())
}

fn classifier(graph: &GraphProto, weights: &Weights) -> TractResult<Vec<u8>> {
  let input = shape(graph, &graph.input[0].name)?;
  let (window_frames, embedding_dim) = (input[1], input[2]);
  let mut writer = Writer::new(KIND_CLASSIFIER, [window_frames, embedding_dim, 0, 0]);
  let mut current = graph.input[0].name.clone();

  for node in &graph.node {
    if !node.input.contains(&current) {
      bail!("{} does not consume the op before it, so the graph branches", node.name);
    }
    match node.op_type.as_str() {
      "Reshape" => {
        let dims = integers(weights, &node.input[1])?;
        if dims.len() != 2 || dims[1] != (window_frames * embedding_dim) as i64 {
          bail!("{} flattens to {dims:?}, not to one window", node.name);
        }
      }
      "Gemm" => {
        if integer(node, "transB").unwrap_or(0) != 1 || integer(node, "transA").unwrap_or(0) != 0 {
          bail!("{} transposes an operand the converter does not expect", node.name);
        }
        if float(node, "alpha").unwrap_or(1.0) != 1.0 || float(node, "beta").unwrap_or(1.0) != 1.0 {
          bail!("{} scales its product, which the runtime does not do", node.name);
        }
        let (shape, values) = floats(weights, &node.input[1])?;
        let [out_dim, in_dim] = shape[..] else {
          bail!("{} has a {:?} weight, not a matrix", node.name, shape)
        };
        let bias = node
          .input
          .get(2)
          .ok_or_else(|| format_err!("{} has no bias", node.name))?;
        writer.op(tag::GEMM);
        writer.usize(out_dim);
        writer.usize(in_dim);
        for input in 0..in_dim {
          for output in 0..out_dim {
            writer.f32(values[output * in_dim + input]);
          }
        }
        writer.floats(&floats(weights, bias)?.1);
      }
      "LayerNormalization" => {
        if integer(node, "axis").unwrap_or(-1) != -1 {
          bail!("{} normalises over an axis other than the last", node.name);
        }
        let (shape, scale) = floats(weights, &node.input[1])?;
        writer.op(tag::LAYER_NORM);
        writer.usize(shape[0]);
        writer.f32(float(node, "epsilon").unwrap_or(1e-5));
        writer.floats(&scale);
        writer.floats(&floats(weights, &node.input[2])?.1);
      }
      "Relu" => writer.op(tag::RELU),
      "Sigmoid" => writer.op(tag::SIGMOID),
      other => bail!("{} is a {other}, which the runtime has no op for", node.name),
    }
    current = node.output[0].clone();
  }

  Ok(writer.finish())
}

fn main() -> TractResult<()> {
  let mut args = std::env::args().skip(1);
  let (source, destination) = match (args.next(), args.next()) {
    (Some(source), Some(destination)) => (source, destination),
    _ => bail!("usage: convert <model.onnx> <model.btww>"),
  };

  let model = tract_onnx::onnx().proto_model_for_path(&source)?;
  let graph = model
    .graph
    .as_ref()
    .ok_or_else(|| format_err!("{source} holds no graph"))?;
  let weights = Weights {
    tensors: graph
      .initializer
      .iter()
      .map(|tensor| (tensor.name.as_str(), tensor))
      .collect(),
    source: &source,
  };

  let convolutional = graph.node.iter().any(|node| node.op_type == "Conv");
  let bytes = if convolutional {
    features(graph, &weights)?
  } else {
    classifier(graph, &weights)?
  };

  std::fs::write(&destination, &bytes)?;
  println!(
    "{destination}: {} kind, {} bytes",
    if convolutional { "features" } else { "classifier" },
    bytes.len()
  );
  Ok(())
}
