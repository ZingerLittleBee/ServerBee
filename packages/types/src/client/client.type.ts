import { Os, Overview, Process, Realtime, SimpleProcess } from "../fusion.type";
import { Result } from "../result.type";
import { TokenPayload } from "../token/index";
import { DeviceDto, DeviceVo } from "./device.type";

export enum ClientStatus {
    ACTIVE = 'ACTIVE',
    INACTIVE = 'INACTIVE',
    DISABLED = 'DISABLED',
    UNKNOWN = 'UNKNOWN'
}

export type CreateClientResult = Result<ClientVo>

export type ClientVo = {
    clientId: string
    name: string
    status: ClientStatus
    lastCommunication: Date
    device?: DeviceVo
}

export type CreateClientVo = ClientVo & { token: string }

type CreateClientDto = {
    name: string
    userId?: string
}

type UpdateClientDto = {
    name?: string
    device?: DeviceDto
}

type FusionDto = {
    overview?: Overview
    os?: Os
    realtime?: Realtime
    simpleProcess?: SimpleProcess[]
    fullProcess?: Process[]
    clientId?: string
    time?: number
}

type ClientPayload = TokenPayload

type UpdateDeviceDto = {
    clientId: string
    device: DeviceDto
}

export type RegisterClientDto = {
    clientId: string
    userId: string
    device: DeviceDto
}


export {
    ClientPayload, CreateClientDto, DeviceDto,
    FusionDto,
    UpdateDeviceDto
};

