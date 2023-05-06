import { useEffect } from "react";
import { useDispatch } from "react-redux";
import Msg from "./Todo";

function App() {
  const dispatch = useDispatch();

  useEffect(() => {
    dispatch({ type: "ws/connect", payload: { url: "ws://" + window.location.host + "/ws" } });

    return () => {
      dispatch({ type: "ws/disconnect", payload: {} });
    };
  }, []);

  return (
    <div>
      <Msg />
    </div>
  );
}

export default App;
