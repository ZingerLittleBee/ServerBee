import { Fusion } from "../fusion.type";

type Report = {
    token: string,
    fusion: Fusion
    time: number
}

type ClientPayload = {
    clientId: string
    userId?: string
}

export { Report, ClientPayload }
