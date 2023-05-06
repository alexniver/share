const encoder = new TextEncoder(); // always utf-8, Uint8Array()
const decoder = new TextDecoder();

export const wsMiddleware = store => {
  let ws = null;


  return next => action => {
    switch (action.type) {
      case 'ws/connect':
        if (ws !== null) {
          ws.close();
        }
        ws = new WebSocket(action.payload.url);
        ws.binaryType = "arraybuffer";

        ws.onopen = () => {
          console.log("websocket open");
        };
        ws.onclose = () => {
          console.log("websocket close");
        };
        ws.onmessage = event => {
          process_msg(store, event.data);
        };
        break;
      case 'ws/sendMsg':
        const method_arr = new Uint8Array(1);
        method_arr[0] = 3;

        let text = action.payload.data;
        const text_arr = encoder.encode(text);
        const text_len_arr = intToArray(text_arr.length);

        // method(1), text_len(4), text(text_len)
        let sendData = new Uint8Array(1 + 4 + text_arr.length);
        let idx = 0;

        sendData.set(new Uint8Array(method_arr), idx);
        idx += 1;

        sendData.set(new Uint8Array(text_len_arr), idx);
        idx += 4;

        sendData.set(new Uint8Array(text_arr), idx);
        ws.send(sendData);
        break;
      case 'ws/sendFile':
        let file = action.payload.data;
        const reader = new FileReader();

        reader.onload = function(e) {
          let file_data = e.target.result;
          const file_data_len_arr = intToArray(file_data.byteLength);
          const method_arr = new Uint8Array(1);
          method_arr[0] = 4;

          const file_name_arr = encoder.encode(file.name);
          const file_name_len_arr = intToArray(file_name_arr.length);

          let sendData = new Uint8Array(1 + 4 + file_name_arr.length + 4 + file_data.byteLength);

          let idx = 0;
          sendData.set(new Uint8Array(method_arr), idx);
          idx += 1;
          sendData.set(new Uint8Array(file_name_len_arr), idx);
          idx += 4;
          sendData.set(new Uint8Array(file_name_arr), idx);
          idx += file_name_arr.length;
          sendData.set(new Uint8Array(file_data_len_arr), idx);
          idx += 4;
          sendData.set(new Uint8Array(file_data), idx);
          ws.send(sendData);
        }

        reader.readAsArrayBuffer(file); // _must_ use ArrayBuffer
        break;

      case 'ws/disconnect':
        if (ws !== null) {
          ws.close();
        }
        ws = null;
        break;
      default:
        return next(action);
    }
  };
};



function process_msg(store, data) {
  let data_view = new DataView(data);
  let method = data_view.getInt8(0, true);

  let i = 1;
  if (method === 61) { // msg list
    while (i < data_view.byteLength) {
      i = deal_msg(store, data_view, i);
    }
  } else if (method === 62) { // one msg
    deal_msg(store, data_view, i);
  } else if (method === 63) { // delete msg
    delete_msg(store, data_view);
  }
}

function deal_msg(store, data_view, i) {
  let [msg, new_i] = parse_msg_data(data_view, i);
  store.dispatch({ type: "ruler/addMsg", payload: { data: msg } });
  return new_i;
}

function delete_msg(store, data_view) {
  let i = 1;
  let id_len = data_view.getInt32(i, true);

  i += 4;
  let id = data_view.getInt32(i, true);
  store.dispatch({ type: "ruler/deleteMsg", payload: { data: id } });
}

function parse_msg_data(data_view, i) {
  let id_len = data_view.getInt32(i, true);
  i += 4;
  let id = data_view.getInt32(i, true);
  i += 4;

  let msg_type_len = data_view.getInt32(i, true);
  i += 4;
  let msg_type = data_view.getInt32(i, true);
  i += 4;

  let text_len = data_view.getInt32(i, true);
  i += 4;
  let text = decoder.decode(data_view.buffer.slice(i, i + text_len));
  i += text_len;
  return [{ id, msg_type, text }, i]
}


// little edian
function intToArray(i) {
  return Uint8Array.of(
    (i & 0x000000ff) >> 0,
    (i & 0x0000ff00) >> 8,
    (i & 0x00ff0000) >> 16,
    (i & 0xff000000) >> 24,
  );
}

// little edian
function arrayToInt(bs, start) {
  start = start || 0;
  const bytes = bs.subarray(start, start + 4).reverse();
  let n = 0;
  for (const byte of bytes.values()) {
    n = (n << 8) | byte;
  }
  return n;
}
