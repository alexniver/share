import { configureStore, getDefaultMiddleware } from '@reduxjs/toolkit'
import rulerReducer from './rulerReducer'
import { wsMiddleware } from './wsMiddleware'

export default configureStore({
  reducer: {
    ruler: rulerReducer,
  },
  middleware: [wsMiddleware, ...getDefaultMiddleware()],
})
