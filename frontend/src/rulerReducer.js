import { createSlice } from '@reduxjs/toolkit'

export const rulerSlice = createSlice({
  name: 'ruler',
  initialState: {
    msg_arr: []
  },
  reducers: {
    addMsg: (state, action) => {
      state.msg_arr.push(action.payload.data);
    },
    deleteMsg: (state, action) => {
      state.msg_arr = state.msg_arr.filter(msg => msg.id !== action.payload.data);
    },
  }
})

// Action creators are generated for each case reducer function
export const { addMsg, deleteMsg } = rulerSlice.actions

export default rulerSlice.reducer
