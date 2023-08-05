import { NestFactory } from '@nestjs/core'
import { StorageModule } from './storage.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'
import { ConfigService } from '@nestjs/config'
import {
    defaultStorageServicePort,
    defaultStorageServiceServerHost,
    kStorageServicePort,
    kStorageServiceServerHost
} from '@server-octopus/shared'

async function bootstrap() {
    const app = await NestFactory.create(StorageModule)
    const configService = app.get(ConfigService)
    const host =
        configService.get<string>(kStorageServiceServerHost) ??
        defaultStorageServiceServerHost
    const port =
        configService.get<number>(kStorageServicePort) ??
        defaultStorageServicePort

    const microservice = app.connectMicroservice<MicroserviceOptions>({
        transport: Transport.TCP,
        options: {
            host,
            port
        }
    })
    await microservice.listen()
}
bootstrap()
